use std::path::Path;

use crate::{
    PostgresDialect, StringSpan,
    sem::{
        AnalysisError, AnalysisProblem, BaseParseContext, CastDefinition, CastInfoRead,
        CreateIndex, CreateSequence, CreateTable, FullName, FunctionInfoRead,
        FunctionOverloadCollection, GetSourcePathWithSpanOpt, IndexInfoRead, MutSourcePathWithSpan,
        OperatorDefinition, OperatorInfoRead, OperatorName, SchemaFileContext, SchemaName,
        SearchPathRead, SemAst, SequenceInfoRead, SourceAccessService, SourcePathWithSpan,
        TableInfoRead, TypeDefinition, TypeInfoRead, TypeReference, ViewInfoRead, ViewLike,
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
    /// Semantic analysis normally continues semantic error and reports to [`crate::sem::ParseContext`].
    ///
    /// Semantic Analysis Error indicates Analyzer Bug.(Unexpected syntax AST structure)
    ///
    #[error(transparent)]
    Sem(#[from] AnalysisError),

    /// Syntax Error
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    SqliteSyn(#[from] ::turso_parser::error::Error),
}

/// load schema dump file
///
/// # Errors
///
/// - [`ParseDumpFileError`]
///
#[expect(dead_code)]
#[deprecated = "use PostgresDialect::parse_schema_file"]
pub fn parse_schema_file<TContext>(
    dump_file: &Path,
    input_context: TContext,
) -> Result<ParsedSchemaDump<TContext>, ParseDumpFileError>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    PostgresDialect::parse_schema_file(dump_file, input_context)
}

///  schema dump file context
#[expect(dead_code)]
#[derive(Debug)]
pub struct ParsedSchemaDump<TContext>(Vec<SemAst>, SchemaFileContext<TContext>)
where
    TContext: BaseParseContext + std::fmt::Debug;

impl<TContext> ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    /// create instance
    pub const fn new(sem_results: Vec<SemAst>, context: SchemaFileContext<TContext>) -> Self {
        Self(sem_results, context)
    }
}

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

impl<TContext> GetSourcePathWithSpanOpt for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan> {
        self.1.get_source_path_with_span()
    }
}

impl<TContext> MutSourcePathWithSpan for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn set_source_path_with_span(&mut self, value: &SourcePathWithSpan) {
        self.1.set_source_path_with_span(value);
    }
}

impl<TContext> BaseParseContext for ParsedSchemaDump<TContext>
where
    TContext: BaseParseContext + std::fmt::Debug,
{
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        self.1.report_problem(problem)?;
        Ok(())
    }

    fn extend(&mut self, extension: Box<dyn BaseParseContext>) {
        self.1.extend(extension);
    }

    fn set_statement_span(&mut self, statement_span: StringSpan) {
        self.1.set_statement_span(statement_span);
    }

    fn get_args(&self) -> &sql_fun_core::SqlFunArgs {
        self.1.get_args()
    }

    fn get_source_file_path(&self) -> Option<&Path> {
        self.1.get_source_file_path()
    }

    fn set_source_file(&mut self, source_access_service: Box<dyn SourceAccessService>) {
        self.1.set_source_file(source_access_service);
    }
}
