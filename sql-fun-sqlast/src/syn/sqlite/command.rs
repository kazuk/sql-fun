#[cfg(feature = "sqlite")]
use crate::syn::sqlite::Stmt;
use crate::{engine_specific::TursoEqpFeature, sem::AnalysisError};

use ::turso_parser::ast::{Cmd as CmdIn, EqpFormat as EqpFormatIn};

/// An owned SQLite command, retaining its EXPLAIN wrapper.
#[derive(Debug)]
pub enum Cmd {
    /// A plain statement.
    Stmt(Stmt),
    /// EXPLAIN with its underlying statement.
    Explain(Stmt),
    /// EXPLAIN QUERY PLAN with its output format.
    ExplainQueryPlan(ExplainQueryPlan),
}

impl Cmd {
    /// Wrap an owned plain statement.
    pub const fn stmt(stmt: Stmt) -> Self {
        Self::Stmt(stmt)
    }

    /// Wrap an owned statement in EXPLAIN.
    pub const fn explain(stmt: Stmt) -> Self {
        Self::Explain(stmt)
    }

    /// Wrap an owned statement in EXPLAIN QUERY PLAN.
    pub const fn explain_query_plan(stmt: Stmt, format: &EqpFormat) -> Self {
        Self::ExplainQueryPlan(ExplainQueryPlan {
            stmt,
            format: *format,
        })
    }

    /// Whether this command is a plain statement.
    pub const fn is_statement(&self) -> bool {
        matches!(self, Self::Stmt(_))
    }

    /// Whether this command is EXPLAIN without QUERY PLAN.
    pub const fn is_explain(&self) -> bool {
        matches!(self, Self::Explain(_))
    }

    /// Borrow the QUERY PLAN payload, if present.
    pub const fn as_explain_query_plan(&self) -> Option<&ExplainQueryPlan> {
        if let Self::ExplainQueryPlan(value) = self {
            Some(value)
        } else {
            None
        }
    }

    /// Borrow the underlying statement without discarding command metadata.
    pub const fn statement(&self) -> &Stmt {
        match self {
            Self::Stmt(stmt) | Self::Explain(stmt) => stmt,
            Self::ExplainQueryPlan(value) => &value.stmt,
        }
    }

    /// Consume the command for analysis of its underlying statement.
    ///
    /// This deliberately discards EXPLAIN metadata at the semantic boundary.
    /// The owned syntax representation retains that metadata until this call.
    pub fn into_statement(self) -> Stmt {
        match self {
            Self::Stmt(stmt) | Self::Explain(stmt) => stmt,
            Self::ExplainQueryPlan(value) => value.stmt,
        }
    }

    /// Consume a plain statement, returning None for either EXPLAIN form.
    pub fn into_plain_statement(self) -> Option<Stmt> {
        if let Self::Stmt(stmt) = self {
            Some(stmt)
        } else {
            None
        }
    }
}

impl From<CmdIn> for Cmd {
    fn from(value: CmdIn) -> Self {
        match value {
            CmdIn::Stmt(stmt) => Self::stmt(Stmt::from(stmt)),
            CmdIn::Explain(stmt) => Self::explain(Stmt::from(stmt)),
            CmdIn::ExplainQueryPlan { stmt, format } => {
                Self::explain_query_plan(Stmt::from(stmt), &EqpFormat::from(format))
            }
        }
    }
}

/// Owned EXPLAIN QUERY PLAN syntax.
#[derive(Debug)]
pub struct ExplainQueryPlan {
    stmt: Stmt,
    format: EqpFormat,
}

impl ExplainQueryPlan {
    /// Borrow the underlying statement.
    pub const fn statement(&self) -> &Stmt {
        &self.stmt
    }

    /// Read the output format, notifying the Turso hook for JSON output.
    pub fn format<Gate: TursoEqpFeature>(
        &self,
        gate: &mut Gate,
    ) -> Result<EqpFormat, AnalysisError> {
        if self.format.is_json() {
            gate.json_eqp_used(self)?;
        }
        Ok(self.format)
    }
}

/// Parser-provided output format for EXPLAIN QUERY PLAN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EqpFormat {
    /// Text output, including the parser's default format.
    Text,
    /// JSON output, a Turso extension.
    Json,
}

impl EqpFormat {
    /// Construct the text format.
    pub const fn text() -> Self {
        Self::Text
    }
    /// Construct the JSON format.
    pub const fn json() -> Self {
        Self::Json
    }
    /// Whether this is JSON output.
    pub const fn is_json(&self) -> bool {
        matches!(self, Self::Json)
    }
}

impl From<EqpFormatIn> for EqpFormat {
    fn from(value: EqpFormatIn) -> Self {
        match value {
            EqpFormatIn::Text => Self::text(),
            EqpFormatIn::Json => Self::json(),
        }
    }
}

#[cfg(test)]
mod test_command_conversion {
    #[cfg(feature = "sqlite")]
    use crate::syn::sqlite::command::{EqpFormat, ExplainQueryPlan};
    #[cfg(feature = "sqlite")]
    use crate::test_helpers::SqliteSynBuilder;
    use crate::{engine_specific::TursoEqpFeature, sem::AnalysisError};

    #[derive(Default)]
    struct Gate(usize);
    impl TursoEqpFeature for Gate {
        fn json_eqp_used(&mut self, _: &ExplainQueryPlan) -> Result<(), AnalysisError> {
            self.0 += 1;
            Ok(())
        }
    }

    struct Reject;
    impl TursoEqpFeature for Reject {
        fn json_eqp_used(&mut self, _: &ExplainQueryPlan) -> Result<(), AnalysisError> {
            Err(std::io::Error::other("JSON EQP denied").into())
        }
    }

    #[test]
    fn reports_json_extension_and_propagates_hook_errors() {
        let cmd = SqliteSynBuilder.parse_command("EXPLAIN QUERY PLAN FORMAT=JSON SELECT 1");
        assert_eq!(
            cmd.as_explain_query_plan()
                .unwrap()
                .format(&mut Reject)
                .unwrap_err()
                .to_string(),
            "JSON EQP denied"
        );
        let cmd = SqliteSynBuilder.parse_command("EXPLAIN QUERY PLAN SELECT 1");
        assert_eq!(
            cmd.as_explain_query_plan()
                .unwrap()
                .format(&mut Reject)
                .unwrap(),
            EqpFormat::text()
        );
    }

    #[rstest::rstest]
    #[case::plain("", None)]
    #[case::explain("EXPLAIN ", None)]
    #[case::default_eqp("EXPLAIN QUERY PLAN ", Some(EqpFormat::text()))]
    #[case::text_eqp("EXPLAIN QUERY PLAN FORMAT=TEXT ", Some(EqpFormat::text()))]
    #[case::json_eqp("EXPLAIN QUERY PLAN FORMAT=JSON ", Some(EqpFormat::json()))]
    fn preserves_command_kind_format_and_statement(
        #[case] prefix: &str,
        #[case] format: Option<EqpFormat>,
    ) {
        let command = {
            let sql = format!("{prefix}CREATE TABLE items (value TEXT)");
            SqliteSynBuilder.parse_command(&sql)
        };
        assert_eq!(command.is_statement(), prefix.is_empty());
        assert_eq!(command.is_explain(), prefix == "EXPLAIN ");
        assert_eq!(
            command
                .statement()
                .as_create_table()
                .unwrap()
                .table_name()
                .name()
                .value(),
            "items"
        );
        let mut gate = Gate::default();
        if let Some(expected) = format {
            let eqp = command.as_explain_query_plan().unwrap();
            assert_eq!(eqp.format(&mut gate).unwrap(), expected);
            assert!(eqp.statement().as_create_table().is_some());
            assert_eq!(gate.0, usize::from(expected.is_json()));
        } else {
            assert!(command.as_explain_query_plan().is_none());
            assert_eq!(gate.0, 0);
        }
        assert!(command.into_statement().as_create_table().is_some());
    }

    #[test]
    fn explain_cannot_be_consumed_as_a_plain_statement() {
        assert!(
            SqliteSynBuilder
                .parse_command("EXPLAIN SELECT 1")
                .into_plain_statement()
                .is_none()
        );
    }
}
