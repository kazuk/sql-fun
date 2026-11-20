//! Unresolved CREATE FOREIGN DATA WRAPPER syntax.

use crate::{
    ast::{FdwOption, Name, postgres::required},
    error::AnalysisError,
    syn::{self, ListOpt, Opt},
};

/// A handler or validator clause. A missing routine name represents explicit `NO`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FdwRoutineClause {
    /// `HANDLER name` or `NO HANDLER`.
    Handler(Option<Vec<String>>),
    /// `VALIDATOR name` or `NO VALIDATOR`.
    Validator(Option<Vec<String>>),
}

/// A PostgreSQL foreign-data wrapper definition before semantic analysis.
#[derive(Clone, Debug)]
pub struct ForeignDataWrapper {
    name: String,
    routines: Vec<FdwRoutineClause>,
    options: Vec<FdwOption>,
}

impl ForeignDataWrapper {
    /// Logical wrapper identifier.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Handler and validator clauses in source order, with qualified routine names
    /// represented as ordered identifier components.
    pub fn routines(&self) -> &[FdwRoutineClause] {
        &self.routines
    }

    /// Generic option name and value pairs in source order.
    pub fn options(&self) -> &[FdwOption] {
        &self.options
    }
}

impl TryFrom<&syn::DefElem> for FdwRoutineClause {
    type Error = AnalysisError;

    fn try_from(value: &syn::DefElem) -> Result<Self, Self::Error> {
        let name = value
            .get_arg()
            .as_inner()
            .map(|arg| {
                let list = required(arg.as_list().as_inner(), "FDW routine name")?;
                Ok::<_, AnalysisError>(Name::try_from(list.get_items())?.0)
            })
            .transpose()?;
        match value.get_defname().as_str() {
            "handler" => Ok(Self::Handler(name)),
            "validator" => Ok(Self::Validator(name)),
            _ => AnalysisError::raise_unexpected_input("FDW routine clause"),
        }
    }
}

impl TryFrom<&syn::CreateFdwStmt> for ForeignDataWrapper {
    type Error = AnalysisError;

    fn try_from(value: &syn::CreateFdwStmt) -> Result<Self, Self::Error> {
        let routines = value
            .get_func_options()
            .as_inner()
            .unwrap_or_default()
            .into_iter()
            .map(|node| {
                FdwRoutineClause::try_from(&required(
                    node.as_def_elem().as_inner(),
                    "FDW routine clause",
                )?)
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            name: value.get_fdwname(),
            routines,
            options: FdwOption::collect(value.get_options())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ast::{
            Dialect,
            FdwRoutineClause::{self, Handler, Validator},
            StatementKind,
        },
        parse_statements::parse_statements,
    };

    #[rstest::rstest]
    #[case::dblink(
        "dblink_fdw VALIDATOR dblink_fdw_validator",
        "dblink_fdw",
        &[Validator(Some(vec!["dblink_fdw_validator".to_owned()]))],
        &[],
    )]
    #[case::omitted("bare", "bare", &[], &[])]
    #[case::no_validator("bare NO VALIDATOR", "bare", &[Validator(None)], &[])]
    #[case::no_handler("bare NO HANDLER", "bare", &[Handler(None)], &[])]
    #[case::handler_only(
        "bare HANDLER handler_fn",
        "bare",
        &[Handler(Some(vec!["handler_fn".to_owned()]))],
        &[],
    )]
    #[case::named_and_explicit(
        r#""My Wrapper" VALIDATOR "My Schema"."My Validator" NO HANDLER OPTIONS ("select" 'a''b', empty '')"#,
        "My Wrapper",
        &[Validator(Some(vec!["My Schema".to_owned(), "My Validator".to_owned()])), Handler(None)],
        &[("select", "a'b"), ("empty", "")],
    )]
    #[case::handler_and_no_validator(
        "bare HANDLER public.handler_fn NO VALIDATOR",
        "bare",
        &[Handler(Some(vec!["public".to_owned(), "handler_fn".to_owned()])), Validator(None)],
        &[],
    )]
    fn retains_wrapper_definition(
        #[case] definition: &str,
        #[case] name: &str,
        #[case] routines: &[FdwRoutineClause],
        #[case] options: &[(&str, &str)],
    ) {
        let sql = format!("CREATE FOREIGN DATA WRAPPER {definition};");
        let statement = parse_statements(&sql, Dialect::postgres())
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(statement.kind(), StatementKind::ForeignDataWrapper);
        let wrapper = statement.foreign_data_wrapper().unwrap();
        assert_eq!(wrapper.name(), name);
        assert_eq!(wrapper.routines(), routines);
        let actual_options: Vec<_> = wrapper
            .options()
            .iter()
            .map(|option| (option.name(), option.value()))
            .collect();
        assert_eq!(actual_options, options);
    }
}
