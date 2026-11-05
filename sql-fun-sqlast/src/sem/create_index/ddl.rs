//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair,
    ast::{IndexExpression as AstIndexExpression, ddl::Index as AstIndex},
    context::ParseContext,
    names::{ColumnName, FullName, LocalName, TableName},
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, CreateIndex, DataSource, FromClause,
        IndexElement, IndexElementCollection, IndexElementWithOrder, IndexStorageChange,
        IndexStorageParameter, ScalarAnalysisContext, SemAst, SemScalarExpr, SortOrder, WithClause,
    },
};

impl CreateIndex {
    #[cfg(test)]
    pub(crate) fn elements_from_ast(
        elements: &[AstIndexExpression],
        context: &mut impl ParseContext,
    ) -> Result<IndexElementCollection, AnalysisError> {
        Self::elements_from_ast_in(elements, context, &ScalarAnalysisContext::default())
    }

    pub(crate) fn elements_from_ast_in(
        elements: &[AstIndexExpression],
        context: &mut impl ParseContext,
        scope: &ScalarAnalysisContext<'_>,
    ) -> Result<IndexElementCollection, AnalysisError> {
        let mut collection = IndexElementCollection::default();
        for element in elements {
            if element.using_order() {
                return AnalysisError::raise_unexpected_input("SortbyUsing in IndexElem");
            }
            let value = if let Some(expr) = &element.expression() {
                IndexElement::from(&SemScalarExpr::from_ast_in(expr, context, scope)?)
            } else {
                IndexElement::from(&ColumnName::from(
                    element.name().as_deref().unwrap_or_default(),
                ))
            };
            let order = if element.descending() == Some(true) {
                SortOrder::Desc
            } else {
                SortOrder::Asc
            };
            collection.push(IndexElementWithOrder::new(&value, order, element.alias()));
        }
        Ok(collection)
    }

    pub(crate) fn from_ast(
        ast: &AstIndex,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        context.begin_parameter_type_analysis();
        let result = Self::from_ast_inner(ast, context);
        context.finish_analysis(result)
    }

    pub(crate) fn from_ast_inner(
        ast: &AstIndex,
        context: &mut impl ParseContext,
    ) -> Result<Option<Self>, AnalysisError> {
        let requested_table = TableName::from(FullName::from_ast_name(ast.relation().name())?);
        let Some(table) = context.resolve_table_name(&requested_table) else {
            context
                .report_analysis_problem(AnalysisProblem::relation_not_found(&requested_table))?;
            return Ok(None);
        };
        let name = FullName::new(
            &table.full_name().catalog().cloned(),
            &table.full_name().schema().cloned(),
            &LocalName::from(ast.name().as_str()),
        );
        let alias = AliasSpec::from_local_name(&table.full_name().local_name());
        let with = WithClause::default();
        let mut from = FromClause::default();
        from.push(DataSource::resolve_relation(
            context,
            table.full_name(),
            &alias,
            &with,
        )?);
        let scope = ScalarAnalysisContext::nested(&with, &from, None);
        let keys = Self::elements_from_ast_in(ast.keys(), context, &scope)?;
        let includes = Self::elements_from_ast_in(ast.includes(), context, &scope)?;
        let predicate = SemScalarExpr::from_ast_optional(ast.predicate(), context, &scope)?;
        let mut index = Self::from_resolved(&name, &table, &keys, &includes, ast.unique());
        index = index.with_if_not_exists(ast.if_not_exists());
        index.set_predicate(predicate);
        if !Self::retain_ast_storage(ast, context, &mut index)? {
            return Ok(None);
        }
        Ok(Some(index))
    }

    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstIndex,
        mut context: TContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(index) = Self::from_ast(ast, &mut context)? else {
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        };
        let context = context.apply_create_index(&index)?;
        Ok(AstAndContextPair::new(
            SemAst::create_index(&index),
            context,
        ))
    }
}

impl CreateIndex {
    pub(crate) fn retain_ast_storage(
        ast: &AstIndex,
        context: &mut impl ParseContext,
        index: &mut Self,
    ) -> Result<bool, AnalysisError> {
        index.set_access_method(ast.access_method());
        if index.access_method() != "btree" {
            return Ok(true);
        }
        let parameters = IndexStorageParameter::from_ast_list(ast.options())?;
        let problems = IndexStorageChange::new(false, &parameters).evaluate(index);
        let mut valid = true;
        for problem in problems {
            if !problem.is_warning() {
                valid = false;
            }
            context.report_analysis_problem(AnalysisProblem::index(&problem))?;
        }
        Ok(valid)
    }
}
