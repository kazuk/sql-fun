use crate::{
    names::AliasName,
    sem::{
        AnalysisError, DataSource, DataSourceRowFunctions, DataSourceRowsFromFunc, FromClause,
        OverloadVariant, PgBuiltInType, SelectStatement, SemScalarExpr, TypeReference, WithClause,
    },
    syn::ListOpt,
    test_helpers::{SynBuilder, TestParseContext, sampling::context as sampling_context},
};

pub(crate) fn source<'a>(query: &'a SelectStatement, alias: &str) -> &'a DataSourceRowFunctions {
    let source = query
        .query_clauses()
        .unwrap()
        .body()
        .source_scope()
        .unwrap()
        .resolve_alias(&AliasName::from(alias))
        .unwrap()
        .0;
    let DataSource::RowFunctions(rows) = source else {
        panic!("ROWS FROM expected")
    };
    rows
}

pub(crate) fn parameter_bindings(query: &SelectStatement) -> Vec<(u32, Option<TypeReference>)> {
    let mut visitor = Parameters::default();
    query.clone().walk(&mut visitor).unwrap();
    visitor.0
}
#[derive(Default)]
struct Parameters(Vec<(u32, Option<TypeReference>)>);
impl crate::sem::traversal::SemanticVisitorMut for Parameters {
    fn expression(&mut self, value: &mut SemScalarExpr) -> Result<(), AnalysisError> {
        if let SemScalarExpr::Param(parameter) = value {
            self.0.push((parameter.position(), value.get_type()));
        }
        Ok(())
    }
}

pub(crate) fn legacy_element(
    expression: &str,
    context: TestParseContext,
) -> DataSourceRowsFromFunc {
    let node = SynBuilder::new().parse(&format!("SELECT {expression}"));
    let value = node
        .as_select_stmt()
        .get_target_list()
        .map(|target| target.as_res_target().get_val())
        .unwrap()
        .swap_remove(0);
    DataSourceRowsFromFunc::analyze_node(
        context,
        &WithClause::default(),
        &FromClause::default(),
        value,
        &::sql_fun_core::IVec::default(),
    )
    .unwrap()
    .0
}

pub(crate) fn deferred_returns_context() -> TestParseContext {
    let mut context = sampling_context();
    let overloads: Vec<_> = [PgBuiltInType::int4(), PgBuiltInType::bool()]
        .into_iter()
        .map(|ty| {
            OverloadVariant::new(
                &Some(ty.clone()),
                &vec![crate::sem::FunctionParam::new_input_param(
                    &Some("n".to_owned()),
                    &Some(ty),
                    &None,
                )],
                false,
                false,
            )
        })
        .collect();
    let name = context.setup_function("pending508", &overloads);
    for overload in &overloads {
        context.set_function_returns_set(&name, overload, true);
        context.set_function_row_property(&name, overload, false);
    }
    context
}
