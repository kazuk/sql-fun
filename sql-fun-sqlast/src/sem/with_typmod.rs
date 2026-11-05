use crate::{
    ast::{Dialect, TypeName},
    sem::{AnalysisError, TypeReference},
};

/// A declaration-local modifier, separate from the wrapped value's identity.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WithTypmod<T> {
    value: T,
    arguments: Vec<String>,
    interpretation: TypmodInterpretation,
}

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
enum TypmodInterpretation {
    Unspecified,
    Applied(i32),
    Unvalidated,
    Invalid(String),
}

impl TypmodInterpretation {
    const fn initial(has_arguments: bool) -> Self {
        if has_arguments {
            Self::Unvalidated
        } else {
            Self::Unspecified
        }
    }
    fn from_result(result: Result<i32, &'static str>) -> Self {
        match result {
            Ok(value) => Self::Applied(value),
            Err(message) => Self::Invalid(message.to_owned()),
        }
    }
    const fn typmod(&self) -> Option<i32> {
        if let Self::Applied(value) = self {
            Some(*value)
        } else {
            None
        }
    }
    const fn is_validated(&self) -> bool {
        matches!(self, Self::Unspecified | Self::Applied(_))
    }
    fn validation_error(&self) -> Option<&str> {
        if let Self::Invalid(message) = self {
            Some(message)
        } else {
            None
        }
    }
}

impl<T> WithTypmod<T> {
    /// The wrapped value, without changing its identity or inference contract.
    pub const fn value(&self) -> &T {
        &self.value
    }
    /// Requested modifier arguments in SQL spelling and order.
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    /// PostgreSQL's encoded modifier, including precision clamping where applicable.
    pub const fn typmod(&self) -> Option<i32> {
        self.interpretation.typmod()
    }
    /// Whether all requested modifiers have been interpreted successfully.
    pub const fn is_validated(&self) -> bool {
        self.interpretation.is_validated()
    }
    /// Why the owning declaration's modifier is invalid, when known.
    pub fn validation_error(&self) -> Option<&str> {
        self.interpretation.validation_error()
    }
    pub(crate) fn new(value: &T, arguments: &[String]) -> Self
    where
        T: Clone,
    {
        Self {
            value: value.clone(),
            arguments: arguments.to_vec(),
            interpretation: TypmodInterpretation::initial(!arguments.is_empty()),
        }
    }
}

impl WithTypmod<TypeReference> {
    pub(crate) fn interpret_postgres(&mut self, resolved: &TypeReference) {
        if self.arguments.is_empty() {
            return;
        }
        // Extension types may define arbitrary typmod input functions. Retain
        // their arguments without claiming that a builtin rule validates them.
        let Some(name) = resolved.full_name() else {
            return;
        };
        if name
            .schema()
            .is_none_or(|schema| schema.as_str() != "pg_catalog")
        {
            return;
        }
        let name = name.local_name();
        if !matches!(
            name.as_str(),
            "varchar"
                | "bpchar"
                | "numeric"
                | "bit"
                | "varbit"
                | "time"
                | "timetz"
                | "timestamp"
                | "timestamptz"
                | "interval"
                | "int2"
                | "int4"
                | "int8"
                | "text"
                | "bool"
                | "date"
                | "float4"
                | "float8"
                | "uuid"
                | "json"
                | "jsonb"
                | "bytea"
        ) {
            return;
        }
        self.interpretation = TypmodInterpretation::from_result(self.builtin_typmod(&name));
    }
}

#[cfg(test)]
mod test_retains_and_interprets_record_typmods {
    use crate::{
        sem::{AnalysisError, ColumnReferenceExpr, SemScalarExpr},
        test_helpers::query::table_function_query,
    };

    #[rstest::rstest]
    #[case::varchar("varchar(3)", 7)]
    #[case::character("character(3)", 7)]
    #[case::array("varchar(3)[]", 7)]
    #[case::qualified("pg_catalog.varchar(3)", 7)]
    #[case::numeric("numeric(12,3)", (12 << 16) + 3 + 4)]
    #[case::negative_scale("numeric(2,-3)", (2 << 16) + 2045 + 4)]
    #[case::larger_scale("numeric(2,4)", (2 << 16) + 4 + 4)]
    #[case::string_precision("numeric('3')", (3 << 16) + 4)]
    #[case::bit("bit(5)", 5)]
    #[case::varbit("bit varying(5)", 5)]
    #[case::timestamp("timestamp(3)", 3)]
    #[case::timestamp_clamping("timestamp(9)", 6)]
    #[case::timetz("time(2) with time zone", 2)]
    #[case::interval("interval day to second(3)", (7176 << 16) + 3)]
    #[case::interval_clamping("interval(9)", (32767 << 16) + 6)]
    #[case::interval_range("interval year to month", (6 << 16) + 65535)]
    #[case::quoted_interval("\"interval\"(32767,3)", (32767 << 16) + 3)]
    fn retains_and_interprets_record_typmods(
        #[case] declared: &str,
        #[case] expected: i32,
        #[values(false, true)] rows: bool,
    ) {
        use crate::{names::AliasName, sem::DataSource};
        let sql = if rows {
            format!("SELECT r.* FROM ROWS FROM(record_fun('') AS(a {declared})) r")
        } else {
            format!("SELECT r.a FROM record_fun('') r(a {declared})")
        };
        let (mut query, context) = table_function_query(&sql, false);
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let source = query
            .query_clauses()
            .unwrap()
            .body()
            .source_scope()
            .unwrap()
            .resolve_alias(&AliasName::from("r"))
            .unwrap()
            .0;
        let restored: DataSource =
            ::serde_json::from_str(&::serde_json::to_string(source).unwrap()).unwrap();
        assert_eq!(&restored, source);
        struct Modifiers {
            expected: i32,
            found: bool,
        }
        impl crate::sem::traversal::SemanticVisitorMut for Modifiers {
            fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
                if let SemScalarExpr::ColumnRef(ColumnReferenceExpr::TableFuncColumn(column)) =
                    value
                {
                    let declaration = column.type_declaration().unwrap();
                    assert_eq!(declaration.typmod(), Some(self.expected));
                    assert!(declaration.is_validated());
                    assert!(!declaration.arguments().is_empty());
                    self.found = true;
                }
                Ok(())
            }
        }
        let mut modifiers = Modifiers {
            expected,
            found: false,
        };
        query.walk(&mut modifiers).unwrap();
        assert!(modifiers.found);
    }

    #[rstest::rstest]
    #[case::zero_numeric("numeric(0)")]
    #[case::quoted_noninteger("numeric('''3''')")]
    #[case::large_numeric("numeric(1001)")]
    #[case::bad_scale("numeric(3,-1001)")]
    #[case::zero_varchar("varchar(0)")]
    #[case::large_varchar("varchar(10485761)")]
    #[case::negative_timestamp("\"timestamp\"(-1)")]
    #[case::zero_bit("bit varying(0)")]
    #[case::no_modifier("pg_catalog.int4(3)")]
    #[case::bad_interval_range("\"interval\"(1000)")]
    fn diagnoses_invalid_record_typmods(#[case] declared: &str) {
        let sql = format!("SELECT r.a FROM record_fun('') r(a {declared})");
        let (_, context) = table_function_query(&sql, false);
        assert!(
            context.has_reported_problem("table_function_invalid_typmod"),
            "{:?}",
            context.reported_problems()
        );
    }

    #[test]
    fn retains_extension_typmods_without_applying_builtin_rules() {
        use crate::{
            names::{AliasName, ColumnName},
            sem::DataSource,
        };
        let (query, context) = table_function_query(
            "SELECT r.a FROM record_fun(src => '') r(a public.custom_type(Point,4326))",
            false,
        );
        assert_eq!(
            context.reported_problem_count(),
            0,
            "{:?}",
            context.reported_problems()
        );
        let source = query
            .query_clauses()
            .unwrap()
            .body()
            .source_scope()
            .unwrap()
            .resolve_alias(&AliasName::from("r"))
            .unwrap()
            .0;
        let DataSource::DeferredTableFunc(source) = source else {
            panic!("deferred invocation")
        };
        let declaration = source
            .output_type_declaration(&ColumnName::from("a"))
            .unwrap();
        assert_eq!(declaration.arguments().len(), 2);
        assert!(!declaration.is_validated());
        assert!(declaration.validation_error().is_none());
    }
}

impl WithTypmod<TypeReference> {
    fn builtin_typmod(&self, name: &str) -> Result<i32, &'static str> {
        // PostgreSQL's ArrayGetIntegerTypmods accepts string/identifier spellings
        // of integers too. Keep the SQL spelling separately from its value.
        let values: Result<Vec<i32>, _> = self
            .arguments
            .iter()
            .map(|argument| Self::integer_argument(argument))
            .collect();
        let values = values.map_err(|_| "type modifiers must be integers")?;
        match (name, values.as_slice()) {
            ("varchar" | "bpchar", [length]) if (1..=10_485_760).contains(length) => Ok(length + 4),
            ("bit" | "varbit", [length]) if (1..=83_886_080).contains(length) => Ok(*length),
            ("numeric", [precision]) => Self::numeric(*precision, 0),
            ("numeric", [precision, scale]) => Self::numeric(*precision, *scale),
            ("time" | "timetz" | "timestamp" | "timestamptz", [precision]) if *precision >= 0 => {
                Ok((*precision).min(6))
            }
            ("interval", [range]) => Self::interval(*range, 65535),
            ("interval", [range, precision]) if *precision >= 0 => {
                Self::interval(*range, (*precision).min(6))
            }
            _ => Err("invalid type modifier arguments or range for declared type"),
        }
    }
    fn integer_argument(argument: &str) -> Result<i32, std::num::ParseIntError> {
        let argument = argument.trim();
        if let Some(inner) = argument.strip_prefix('\'')
            && let Some(inner) = inner.strip_suffix('\'')
        {
            inner.trim().parse()
        } else if let Some(inner) = argument.strip_prefix('"')
            && let Some(inner) = inner.strip_suffix('"')
        {
            inner.trim().parse()
        } else {
            argument.parse()
        }
    }
    fn numeric(precision: i32, scale: i32) -> Result<i32, &'static str> {
        if !(1..=1000).contains(&precision) || !(-1000..=1000).contains(&scale) {
            return Err("numeric precision must be 1..1000 and scale must be -1000..1000");
        }
        Ok(((precision << 16) | (scale & 0x7ff)) + 4)
    }
    fn interval(range: i32, precision: i32) -> Result<i32, &'static str> {
        if ![
            4, 2, 8, 1024, 2048, 4096, 6, 1032, 3080, 7176, 3072, 7168, 6144, 32767,
        ]
        .contains(&range)
        {
            return Err("invalid interval field range");
        }
        if range == 32767 && precision == 65535 {
            Ok(-1)
        } else {
            Ok((range << 16) | precision)
        }
    }
}

impl WithTypmod<TypeReference> {
    pub(crate) fn from_ast_declaration(
        ast: &TypeName,
        resolved: Option<&TypeReference>,
    ) -> Result<Self, AnalysisError> {
        let mut declaration = Self::new(
            &TypeReference::from_ast(ast)?,
            &Self::ast_modifier_arguments(ast)?,
        );
        if let Some(resolved) = resolved {
            declaration.interpret_postgres(resolved);
        }
        Ok(declaration)
    }
}

impl WithTypmod<TypeReference> {
    pub(crate) fn ast_modifier_arguments(ast: &TypeName) -> Result<Vec<String>, AnalysisError> {
        use crate::ast::deparse::{SqlEmit, SqlWriter};
        let mut arguments = Vec::new();
        for modifier in ast.modifiers() {
            let mut writer = SqlWriter::new(Dialect::postgres());
            modifier
                .emit(&mut writer)
                .map_err(|error| AnalysisError::unexpected_input_error(&error.to_string()))?;
            arguments.push(writer.into_sql());
        }
        Ok(arguments)
    }
}
