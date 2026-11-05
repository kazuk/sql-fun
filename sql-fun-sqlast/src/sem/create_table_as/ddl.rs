//! Analysis owned by the semantic definitions in this module.

use crate::{
    AstAndContextPair, CacheContext,
    ast::{CreateTableMode, ddl::TableAs as AstTableAs},
    context::ParseContext,
    names::{FullName, TableName},
    sem::{
        AnalysisError, AnalysisProblem, CreateTable, CreateTableAs, SelectStatement, SemAst,
        SourcePathWithSpan, problem::CreationProblem,
    },
};

impl CreateTableAs {
    pub(crate) fn from_ast_and_apply<TContext: ParseContext>(
        ast: &AstTableAs,
        context: TContext,
        cache: &mut CacheContext,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let Some(source) = context.get_source_path_with_span().cloned() else {
            return AnalysisError::raise_unexpected_none("source span");
        };
        Self::from_ast_and_apply_at(ast, context, cache, &source)
    }

    pub(crate) fn from_ast_and_apply_at<TContext: ParseContext>(
        ast: &AstTableAs,
        mut context: TContext,
        cache: &mut CacheContext,
        source: &SourcePathWithSpan,
    ) -> Result<AstAndContextPair<TContext>, AnalysisError> {
        let mut query = SelectStatement::from_ast(&ast.query, &mut context, cache)?;
        let name = FullName::from_ast_name(&ast.into.relation.name)?;
        if ast.into.columns.len() > query.result_columns().len() {
            context.report_problem(AnalysisProblem::creation(CreationProblem::named(
                &name,
                "too many output column names",
                source.span(),
            )))?;
            return Ok(AstAndContextPair::new(
                SemAst::unresolved_statement(),
                context,
            ));
        }
        query.rename_columns(&ast.into.columns);
        let columns = query.result_columns().column_definitions()?;
        let mut names = Vec::new();
        for column in &columns {
            if names.contains(&column.name()) {
                context.report_problem(AnalysisProblem::creation(CreationProblem::named(
                    &name,
                    "duplicate output column name",
                    source.span(),
                )))?;
                return Ok(AstAndContextPair::new(
                    SemAst::unresolved_statement(),
                    context,
                ));
            }
            names.push(column.name());
        }
        let table = CreateTable::new(
            &TableName::from(name.clone()),
            columns,
            CreateTableMode::from_if_not_exists(ast.if_not_exists),
            ast.into.relation.persistence == "t",
            source,
        );
        let table_as = Self::new(&name, &SemAst::select(&query), ast.materialized)
            .with_if_not_exists(ast.if_not_exists);
        let context = if ast.materialized {
            context.apply_create_table_as(&table_as)?
        } else {
            context.apply_create_table(&table)?
        };
        Ok(AstAndContextPair::new(
            SemAst::create_table_as(&table_as),
            context,
        ))
    }
}
