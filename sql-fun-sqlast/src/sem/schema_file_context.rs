mod cast_info;
mod func_info;
mod sequence_info;
mod table_info;
mod type_info;
mod view_info;

use std::path::{Path, PathBuf};

use sql_fun_core::SqlFunArgs;

use self::{
    func_info::FunctionDefinitionCollection, sequence_info::SequenceInfoCollection,
    table_info::TableDefinitionCollection, type_info::TypeDefinitionCollection,
    view_info::ViewInformationCollection,
};

use crate::{
    StringSpan, TrieMap,
    sem::{
        AnalysisError, AnalysisProblem, BaseParseContext, CastDefinition, Comment, CreateExtension,
        CreateIndex, CreateSchema, GetSourcePathWithSpanOpt, ImplicitChange, MutSourcePathWithSpan,
        ObjectKind, OperatorDefinition, OperatorInfoRead, OperatorName, ParseContext, SchemaName,
        SemScalarExpr, SourceAccessService, VariableSet,
        parse_context::{IndexInfoRead, SearchPathRead},
    },
};

use super::SourcePathWithSpan;

/// schema file based context
#[derive(Debug)]
pub struct SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    base: TBaseContext,
    types: TypeDefinitionCollection,
    tables: TableDefinitionCollection,
    functions: FunctionDefinitionCollection,
    extensions: TrieMap<CreateExtension>,
    variables: TrieMap<SemScalarExpr>,
    schemas: TrieMap<CreateSchema>,
    sequences: SequenceInfoCollection,
    comments: TrieMap<(ObjectKind, String)>,
    views: ViewInformationCollection,
    casts: TrieMap<TrieMap<CastDefinition>>,
    binary_operators: TrieMap<OperatorDefinition>,
    left_unary_operators: TrieMap<OperatorDefinition>,
    right_unary_operators: TrieMap<OperatorDefinition>,
    indexes: TrieMap<CreateIndex>,
    statement_span: StringSpan,
    sql_source_path: PathBuf,
    source_access_service: Option<Box<dyn SourceAccessService>>,
    reports: Vec<String>,
    source: Option<SourcePathWithSpan>,
}

impl<TBaseContext> SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    /// create context for loading schema file
    pub fn new(sql_source_path: &Path, base: TBaseContext) -> Self {
        Self {
            base,
            sql_source_path: sql_source_path.to_path_buf(),
            types: TypeDefinitionCollection::default(),
            tables: TableDefinitionCollection::default(),
            functions: FunctionDefinitionCollection::default(),
            extensions: TrieMap::default(),
            variables: TrieMap::default(),
            schemas: TrieMap::default(),
            sequences: SequenceInfoCollection::default(),
            comments: TrieMap::default(),
            views: ViewInformationCollection::default(),
            casts: TrieMap::default(),
            binary_operators: TrieMap::default(),
            left_unary_operators: TrieMap::default(),
            right_unary_operators: TrieMap::default(),
            statement_span: StringSpan::default(),
            indexes: TrieMap::default(),
            source_access_service: None,
            reports: Default::default(),
            source: None,
        }
    }

    /// apply index from constraint
    pub fn apply_implicit_change(
        self,
        implicit_change: ImplicitChange,
    ) -> Result<Self, AnalysisError> {
        match implicit_change {
            ImplicitChange::CreateIndex(create_index) => self.apply_create_index(&create_index),
        }
    }
}

impl<TBaseContext> GetSourcePathWithSpanOpt for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }
}

impl<TBaseContext> MutSourcePathWithSpan for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn set_source_path_with_span(&mut self, value: &SourcePathWithSpan) {
        self.source = Some(value.clone())
    }
}

#[cfg(test)]
mod test_schema_file_context {
    use std::path::Path;

    use crate::{
        sem::SchemaFileContext,
        test_helpers::{TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_new(test_context: TestParseContext) {
        let path = Path::new("hoge.sql");
        let _context = SchemaFileContext::new(path, test_context);
    }
}

impl<TBaseContext> IndexInfoRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_index(&self, index_name: &str) -> Option<&CreateIndex> {
        if let Some(index) = self.indexes.get(index_name) {
            Some(index)
        } else {
            self.base.get_index(index_name)
        }
    }
}

impl<TBaseContext> OperatorInfoRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn get_operator_op(
        &self,
        name: &OperatorName,
        has_left: bool,
        has_right: bool,
    ) -> Option<&OperatorDefinition> {
        if !name.is_valid() {
            return None;
        }

        let name_str = &name.to_name_string();
        let Some(ope) = (match (has_left, has_right) {
            (true, true) => self.binary_operators.get(name_str),
            (true, false) => self.left_unary_operators.get(name_str),
            (false, true) => self.right_unary_operators.get(name_str),
            _ => panic!("accessing no-param operator"),
        }) else {
            return self.base.get_operator_op(name, has_left, has_right);
        };
        Some(ope)
    }
}

impl<TBaseContext> SearchPathRead for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn set_search_path(&mut self, search_path: Vec<SchemaName>) {
        self.base.set_search_path(search_path);
    }

    fn get_default_schema(&self) -> Option<SchemaName> {
        self.base.get_default_schema()
    }

    fn get_search_path(&self) -> &[SchemaName] {
        self.base.get_search_path()
    }
}

impl<TBaseContext> BaseParseContext for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
        self.base.metadata()
    }

    fn get_args(&self) -> &SqlFunArgs {
        self.base.get_args()
    }

    fn get_source_file_path(&self) -> Option<&Path> {
        Some(self.sql_source_path.as_path())
    }

    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        if let Some(source_provider) = &self.source_access_service {
            let source = source_provider.get_source_span(
                self.get_args(),
                self.sql_source_path.as_path(),
                self.statement_span,
            )?;
            let theme = self.base.get_args().highlighter_theme()?;

            let report = problem.render_report(&theme, source)?;
            eprintln!("{report}");
            self.reports.push(report)
        }

        self.base.report_problem(problem)
    }

    fn extend(&mut self, extension: Box<dyn BaseParseContext>) {
        self.base.extend(extension);
    }

    fn set_statement_span(&mut self, statement_span: StringSpan) {
        self.statement_span = statement_span;
        self.source = Some(SourcePathWithSpan::new(
            &self.sql_source_path,
            &statement_span,
        ));
    }

    fn set_source_file(&mut self, source_access_service: Box<dyn super::SourceAccessService>) {
        self.source_access_service = Some(source_access_service);
    }
}

impl<TBaseContext> ParseContext for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn apply_create_extension(
        mut self,
        create_extension: &CreateExtension,
    ) -> Result<Self, AnalysisError> {
        let name = create_extension.name();
        self.extensions.insert(name, create_extension.clone());
        Ok(self)
    }

    fn apply_variable_set(mut self, variable_set: &VariableSet) -> Result<Self, AnalysisError> {
        let name = variable_set.name();
        self.variables.insert(name, variable_set.value().clone());
        Ok(self)
    }

    fn apply_create_schema(mut self, create_schema: &CreateSchema) -> Result<Self, AnalysisError> {
        let name = create_schema.name();
        self.schemas.insert(name, create_schema.clone());
        Ok(self)
    }

    fn apply_comment(mut self, comment: &Comment) -> Result<Self, AnalysisError> {
        let target = comment.target();
        self.comments.insert(
            target,
            (*comment.object_type(), String::from(comment.comment())),
        );
        Ok(self)
    }

    fn apply_create_index(mut self, create_index: &CreateIndex) -> Result<Self, AnalysisError> {
        let name = create_index.name();
        self.indexes.insert(name, create_index.clone());
        Ok(self)
    }
}
