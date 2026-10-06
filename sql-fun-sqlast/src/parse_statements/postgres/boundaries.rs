//! Scanner candidates corrected by local boundary rules, never a SQL validator.
//!
//! Add syntax support in `BoundaryState` and its corpus tests. Candidate discovery,
//! iteration, and error classification do not depend on particular SQL constructs.

use crate::StringSpan;
use crate::parse_statements::ParseStatementError;

use ::pg_query::protobuf::Token;

pub(super) fn discover(sql: &str) -> Vec<Result<StringSpan, ParseStatementError>> {
    if let Ok(statements) = ::pg_query::split_with_parser(sql) {
        return statements
            .into_iter()
            .map(|statement| Ok(source_span(sql, statement)))
            .collect();
    }
    let candidates = match ::pg_query::split_with_scanner(sql) {
        Ok(candidates) => candidates,
        Err(error) => {
            return vec![Err(ParseStatementError::boundary_discovery(
                StringSpan::new(0, sql.len()),
                &error.to_string(),
            ))];
        }
    };
    let scanned = match ::pg_query::scan(sql) {
        Ok(scanned) => scanned,
        Err(error) => {
            return vec![Err(ParseStatementError::boundary_discovery(
                StringSpan::new(0, sql.len()),
                &error.to_string(),
            ))];
        }
    };
    let mut candidates = CandidateCoverage::new(sql, &candidates);
    let mut state = BoundaryState::default();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut substantive = false;
    for scanned in scanned.tokens {
        let token = scanned.token();
        if token == Token::SqlComment || token == Token::CComment {
            continue;
        }
        let offset = scanned.start as usize;
        if token == Token::Ascii59 {
            // Scanner ends are the primary candidates. Only supplement gaps,
            // where the splitter may have discarded keyword-free invalid input.
            let candidate = candidates.is_candidate(offset);
            state.separator();
            if candidate && state.is_outer_boundary() {
                if substantive {
                    ranges.push(Ok(StringSpan::new(start, offset - start)));
                }
                start = scanned.end as usize;
                substantive = false;
            }
        } else {
            substantive = true;
            state.observe(token);
        }
        if state.is_unknown() {
            ranges.push(Err(ParseStatementError::boundary_discovery(
                StringSpan::new(start, sql.len() - start),
                "PostgreSQL statement boundary cannot be established after unmatched nesting",
            )));
            return ranges;
        }
    }
    if !state.is_outer_boundary() {
        ranges.push(Err(ParseStatementError::boundary_discovery(
            StringSpan::new(start, sql.len() - start),
            "PostgreSQL statement boundary cannot be established with unclosed nesting",
        )));
    } else if substantive {
        ranges.push(Ok(StringSpan::new(start, sql.len() - start)));
    }
    ranges
}

#[cfg(test)]
mod test_discover {
    #[rstest::rstest]
    #[case::function(
        "CREATE FUNCTION f() RETURNS integer LANGUAGE SQL BEGIN ATOMIC SELECT 1; RETURN 2; END"
    )]
    #[case::procedure("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT 1; SELECT 2; END")]
    #[case::empty_body("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC END")]
    #[case::replace(
        "CREATE OR REPLACE FUNCTION f() RETURNS integer BEGIN /* ; */ ATOMIC SELECT 1; END"
    )]
    #[case::case_expression(
        "CREATE FUNCTION f() RETURNS integer LANGUAGE SQL BEGIN ATOMIC SELECT CASE WHEN true THEN CASE WHEN false THEN 1 ELSE 2 END ELSE 3 END; END"
    )]
    #[case::nested_routine(
        "CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC CREATE FUNCTION f() RETURNS int LANGUAGE SQL BEGIN ATOMIC SELECT 1; END; SELECT 2; END"
    )]
    #[case::rule("CREATE RULE r AS ON INSERT TO t DO (SELECT 1; SELECT 2;)")]
    #[case::dollar_quote(
        "CREATE FUNCTION f() RETURNS int AS $fn$ SELECT ';'; SELECT 2; $fn$ LANGUAGE SQL"
    )]
    #[case::array("SELECT ARRAY[1, 2]")]
    #[case::transaction("BEGIN")]
    #[case::transaction_end("END")]
    #[case::identifier_keywords("SELECT begin atomic")]
    fn legal_construct_remains_one_range_during_recovery(
        #[case] statement: &str,
        #[values(false, true)] malformed_before: bool,
    ) {
        assert_eq!(
            ::pg_query::parse(statement).unwrap().protobuf.stmts.len(),
            1
        );
        let sql = if malformed_before {
            format!("asdf;{statement};SELECT 3;")
        } else {
            format!("{statement};asdf;SELECT 3;")
        };
        let ranges = super::discover(&sql);
        assert_eq!(ranges.len(), 3, "{sql}");
        let index = usize::from(malformed_before);
        assert_eq!(
            ranges[index].as_ref().unwrap().get_from(&sql),
            Some(statement)
        );
        assert_eq!(ranges[2].as_ref().unwrap().get_from(&sql), Some("SELECT 3"));
    }

    #[rstest::rstest]
    #[case::syntax("SELECT FROM")]
    #[case::no_keyword("asdf")]
    #[case::operator("+")]
    #[case::quoted_identifier("\"日本語\"")]
    #[case::compound("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT FROM; SELECT 2; END")]
    #[case::misplaced_end(
        "CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT END; SELECT 2; END"
    )]
    #[case::unclosed_case("SELECT CASE WHEN true THEN 1")]
    #[case::unclosed_case_in_body(
        "CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT CASE WHEN true THEN 1; END"
    )]
    fn malformed_closed_range_does_not_absorb_next_statement(#[case] statement: &str) {
        let sql = format!("SELECT 1;{statement};SELECT 3;");
        let ranges = super::discover(&sql);
        assert_eq!(ranges.len(), 3);
        assert_eq!(ranges[1].as_ref().unwrap().get_from(&sql), Some(statement));
        assert!(::pg_query::parse(statement).is_err());
        assert_eq!(ranges[2].as_ref().unwrap().get_from(&sql), Some("SELECT 3"));
    }

    #[rstest::rstest]
    #[case::round("SELECT (1; SELECT 3;")]
    #[case::square("SELECT ARRAY[1; SELECT 3;")]
    #[case::mismatch("SELECT (1]; SELECT 3;")]
    #[case::unmatched("SELECT 1); SELECT 3;")]
    #[case::compound("CREATE PROCEDURE p() LANGUAGE SQL BEGIN ATOMIC SELECT 1; SELECT 3;")]
    fn uncertain_nesting_preserves_prefix_and_reports_unresolved_tail(#[case] tail: &str) {
        let sql = format!("SELECT 1;{tail}");
        let mut ranges = super::discover(&sql).into_iter();
        assert_eq!(
            ranges.next().unwrap().unwrap().get_from(&sql),
            Some("SELECT 1")
        );
        let error = ranges.next().unwrap().unwrap_err();
        assert!(error.is_boundary_discovery());
        assert!(!error.is_recoverable());
        assert_eq!(error.span().get_from(&sql), Some(tail));
        assert!(ranges.next().is_none());
    }

    #[test]
    fn parser_fast_path_preserves_authoritative_ranges() {
        let sql =
            " ; /* ; */ CREATE RULE r AS ON INSERT TO t DO (SELECT 1; SELECT 2;); BEGIN; END;";
        let expected = ::pg_query::split_with_parser(sql).unwrap();
        let actual: Vec<_> = super::discover(sql)
            .into_iter()
            .map(|range| range.unwrap().get_from(sql).unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
}

fn source_span(sql: &str, statement: &str) -> StringSpan {
    StringSpan::new(
        statement.as_ptr() as usize - sql.as_ptr() as usize,
        statement.len(),
    )
}

/// Coverage supplied by the scanner. Internal separators already covered by a
/// candidate stay internal; uncovered separators can recover skipped input.
struct CandidateCoverage {
    ranges: Vec<StringSpan>,
    index: usize,
}

impl CandidateCoverage {
    fn new(sql: &str, statements: &[&str]) -> Self {
        let mut ranges = Vec::new();
        for statement in statements {
            ranges.push(source_span(sql, statement));
        }
        Self { ranges, index: 0 }
    }

    fn is_candidate(&mut self, offset: usize) -> bool {
        while let Some(range) = self.ranges.get(self.index) {
            if range.end_pos() < offset {
                self.index += 1;
            } else {
                return !((range.end_pos() - range.len()) <= offset && offset < range.end_pos());
            }
        }
        true
    }
}

/// Minimal corrective knowledge for separators inside SQL routine bodies and
/// nesting. An unknown state terminates recovery; parsing errors never request
/// extension. Closing a known construct makes its next outer separator usable
/// even when the construct itself is malformed.
#[derive(Default)]
struct BoundaryState {
    nesting: Vec<Nesting>,
    prefix: RoutinePrefix,
    routine_depth: usize,
    previous: Option<Token>,
    unknown: bool,
}

impl BoundaryState {
    fn observe(&mut self, token: Token) {
        let at_statement_start = self.prefix.is_start();
        self.prefix.observe(token);
        if self.prefix.just_recognized_routine() {
            self.routine_depth = self.nesting.len();
        }
        if token == Token::Ascii40 || token == Token::Ascii91 {
            self.nesting.push(Nesting::delimiter(token));
        } else if token == Token::Ascii41 || token == Token::Ascii93 {
            if !self
                .nesting
                .pop()
                .is_some_and(|opening| opening.closes_with(token))
            {
                self.unknown = true;
            }
        } else if token == Token::Case {
            self.nesting.push(Nesting::case());
        } else if token == Token::Atomic
            && self.previous == Some(Token::BeginP)
            && self.prefix.is_routine()
            && self.routine_depth == self.nesting.len()
        {
            self.nesting.push(Nesting::atomic());
            self.prefix = RoutinePrefix::default();
        } else if token == Token::EndP
            && self
                .nesting
                .last()
                .is_some_and(|opening| opening.closes_with_end(at_statement_start))
        {
            // CASE ends an expression. A routine END must occur between body
            // statements; a misplaced END in malformed SQL must not expose the
            // rest of that body as separate top-level statements. A standalone
            // transaction END does not close anything.
            self.nesting.pop();
        }
        self.previous = Some(token);
    }

    fn separator(&mut self) {
        // SQL CASE is an expression, not a semicolon-delimited procedural
        // statement. An unclosed CASE cannot justify extending a failed range
        // across a separator. Keep only nesting that can contain statements.
        while self.nesting.last().is_some_and(Nesting::is_case) {
            self.nesting.pop();
        }
        self.prefix = RoutinePrefix::default();
        self.previous = None;
    }

    const fn is_outer_boundary(&self) -> bool {
        !self.unknown && self.nesting.is_empty()
    }

    const fn is_unknown(&self) -> bool {
        self.unknown
    }
}

#[derive(Clone, Copy)]
enum Nesting {
    Delimiter(Token),
    Atomic,
    Case,
}

impl Nesting {
    const fn delimiter(token: Token) -> Self {
        Self::Delimiter(token)
    }

    const fn atomic() -> Self {
        Self::Atomic
    }

    const fn case() -> Self {
        Self::Case
    }

    fn closes_with(self, token: Token) -> bool {
        match self {
            Self::Delimiter(opening) => {
                (opening == Token::Ascii40 && token == Token::Ascii41)
                    || (opening == Token::Ascii91 && token == Token::Ascii93)
            }
            Self::Atomic | Self::Case => false,
        }
    }

    const fn is_case(&self) -> bool {
        matches!(self, Self::Case)
    }

    const fn closes_with_end(&self, at_statement_start: bool) -> bool {
        match self {
            Self::Atomic => at_statement_start,
            Self::Case => true,
            Self::Delimiter(_) => false,
        }
    }
}

/// Recognize only the CREATE [OR REPLACE] FUNCTION/PROCEDURE prefix, at the
/// start of an outer or routine-body statement. No routine options are parsed.
#[derive(Default)]
enum RoutinePrefix {
    #[default]
    Start,
    Create,
    Or,
    Replace,
    Recognized,
    Routine,
    Other,
}

impl RoutinePrefix {
    fn observe(&mut self, token: Token) {
        *self = match self {
            Self::Start if token == Token::Create => Self::Create,
            Self::Create if token == Token::Or => Self::Or,
            Self::Or if token == Token::Replace => Self::Replace,
            Self::Create | Self::Replace
                if token == Token::Function || token == Token::Procedure =>
            {
                Self::Recognized
            }
            Self::Recognized | Self::Routine => Self::Routine,
            _ => Self::Other,
        };
    }

    const fn is_start(&self) -> bool {
        matches!(self, Self::Start)
    }

    const fn just_recognized_routine(&self) -> bool {
        matches!(self, Self::Recognized)
    }

    const fn is_routine(&self) -> bool {
        matches!(self, Self::Recognized | Self::Routine)
    }
}
