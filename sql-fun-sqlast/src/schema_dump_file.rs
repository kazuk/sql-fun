use std::path::Path;

use crate::{
    AstAndContextPair, StringSpan, parse, read_sql_file,
    sem::{
        AnalysisError, AnalysisProblem, BaseParseContext, CastDefinition, CastInfoRead,
        CreateIndex, CreateSequence, CreateTable, FullName, FunctionInfoRead,
        FunctionOverloadCollection, IndexInfoRead, OperatorDefinition, OperatorInfoRead,
        OperatorName, SchemaFileContext, SchemaName, SearchPathRead, SemAst, SequenceInfoRead,
        TableInfoRead, TypeDefinition, TypeInfoRead, TypeReference, ViewInfoRead, ViewLike,
        semantic_analysis,
    },
};
/// Error for reading schema dump
#[derive(Debug, thiserror::Error)]
pub enum ParseDumpFileError {
    /// IO Error
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Syntax Error
    #[error(transparent)]
    Syn(#[from] ::pg_query::Error),

    /// Semantic Analysis Error
    ///
    /// # Note
    ///
    /// Semantic analysis normaly continues semantic error and reports to [crate::sem::ParseContext].
    ///
    /// Semantic Analysis Error indicates Analyzer Bug.(Unexpected syntax AST structure)
    ///
    #[error(transparent)]
    Sem(#[from] AnalysisError),
}

/// load schema dump file
pub fn parse_schema_file<TContext>(
    dump_file: &Path,
    input_context: TContext,
) -> Result<ParsedSchemaDump<TContext>, ParseDumpFileError>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    let sql_statements = read_sql_file(dump_file)?;

    let statements = ::pg_query::split_with_parser(&sql_statements)?;
    let mut context = SchemaFileContext::new(input_context);
    let mut sem_results = Vec::new();
    for statement in statements {
        context.set_statement_span(StringSpan::from_str_in_str(&sql_statements, statement));
        let syn = parse(statement)?;
        let AstAndContextPair(sem, rcontext) = semantic_analysis(context, syn)?;
        sem_results.push(sem);
        context = rcontext
    }
    Ok(ParsedSchemaDump(sem_results, context))
}

///  schema dump file context
#[expect(dead_code)]
#[derive(Debug)]
pub struct ParsedSchemaDump<TContext>(Vec<SemAst>, SchemaFileContext<TContext>)
where
    TContext: BaseParseContext + std::fmt::Debug;

impl<TContext> TypeInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.1.get_type_impl(key)
    }

    fn get_array_item_type(&self, _type_name: &TypeReference) -> Option<TypeReference> {
        todo!()
    }
}

impl<TContext> TableInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable> {
        self.1.get_table_impl(key)
    }
}

impl<TContext> ViewInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
        self.1.get_view_impl(key)
    }
}

impl<TContext> IndexInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_index(&self, index_name: &str) -> Option<&CreateIndex> {
        self.1.get_index(index_name)
    }
}

impl<TContext> SequenceInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_sequence_impl(&self, key: &FullName) -> Option<&CreateSequence> {
        self.1.get_sequence_impl(key)
    }
}

impl<TContext> FunctionInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        self.1.get_function_impl(name)
    }
}

impl<TContext> CastInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_explicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        self.1.get_explicit_cast(source_type, target_type)
    }

    fn get_implicit_cast(
        &self,
        source_type: &TypeReference,
        target_type: &TypeReference,
    ) -> Option<CastDefinition> {
        self.1.get_implicit_cast(source_type, target_type)
    }

    fn get_implicit_castable(
        &self,
        source_type: &TypeReference,
    ) -> std::collections::HashSet<TypeReference> {
        self.1.get_implicit_castable(source_type)
    }
}

impl<TContext> OperatorInfoRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_operator_op(
        &self,
        name: &OperatorName,
        has_left: bool,
        has_right: bool,
    ) -> Option<&OperatorDefinition> {
        self.1.get_operator_op(name, has_left, has_right)
    }
}

impl<TContext> SearchPathRead for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn set_search_path(&mut self, search_path: Vec<SchemaName>) {
        self.1.set_search_path(search_path);
    }

    fn get_default_schema(&self) -> Option<SchemaName> {
        self.1.get_default_schema()
    }

    fn get_search_path(&self) -> &[SchemaName] {
        self.1.get_search_path()
    }
}

impl<TContext> BaseParseContext for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn report_problem(&mut self, _problem: AnalysisProblem) {
        todo!()
    }

    fn extend(&mut self, _extension: Box<dyn BaseParseContext>) {
        todo!()
    }

    fn set_statement_span(&mut self, statement_span: StringSpan) {
        self.1.set_statement_span(statement_span)
    }
}
