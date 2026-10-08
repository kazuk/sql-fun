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
        CreateIndex, CreateSchema, DiagnosticContext, GetSourcePathWithSpanOpt, ImplicitChange,
        MutSourcePathWithSpan, ObjectKind, OperatorDefinition, OperatorInfoRead, OperatorName,
        ParseContext, SchemaName, SemScalarExpr, SourceAccessService, VariableSet,
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
    search_path: Vec<SchemaName>,
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
    problems: Vec<AnalysisProblem>,
    source: Option<SourcePathWithSpan>,
}

impl<TBaseContext> SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    /// create context for loading schema file
    ///
    /// The initial search path is copied from the supplied base. Later changes
    /// to this context's search path leave the base unchanged.
    pub fn new(sql_source_path: &Path, base: TBaseContext) -> Self {
        let search_path = base.get_search_path().to_vec();
        Self {
            base,
            search_path,
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
            problems: Vec::new(),
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
    fn get_default_schema(&self) -> Option<SchemaName> {
        self.search_path.first().cloned()
    }

    fn get_search_path(&self) -> &[SchemaName] {
        &self.search_path
    }
}

impl<TBaseContext> DiagnosticContext for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
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

        self.problems.push(problem);
        Ok(())
    }
}

#[cfg(test)]
mod test_diagnostic_context {
    use crate::StringSpan;
    use crate::sem::{AnalysisProblem, DiagnosticContext, FullName, SchemaFileContext};
    use crate::test_helpers::TestParseContext;
    use std::path::Path;

    #[test]
    fn collects_problems_without_reporting_to_base() -> testresult::TestResult {
        let mut base = TestParseContext::default();
        base.reject_diagnostics_after(0);
        let mut context = SchemaFileContext::new(Path::new("schema.sql"), base);
        let first = AnalysisProblem::function_not_found(
            &FullName::from_local_name("first"),
            &StringSpan::new(0, 5),
        );
        let second = AnalysisProblem::function_not_found(
            &FullName::from_local_name("second"),
            &StringSpan::new(6, 6),
        );

        context.report_problem(first)?;
        context.report_problem(second)?;

        assert_eq!(context.problems.len(), 2);
        assert!(context.base.reported_problems().is_empty());
        Ok(())
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
}

impl<TBaseContext> ParseContext for SchemaFileContext<TBaseContext>
where
    TBaseContext: BaseParseContext + std::fmt::Debug,
{
    fn set_search_path(&mut self, search_path: &[SchemaName]) {
        self.search_path = search_path.to_vec();
    }

    fn set_statement_span(&mut self, statement_span: StringSpan) {
        self.statement_span = statement_span;
        self.source = Some(SourcePathWithSpan::new(
            &self.sql_source_path,
            &statement_span,
        ));
    }

    fn set_source_file(&mut self, source_access_service: Box<dyn SourceAccessService>) {
        self.source_access_service = Some(source_access_service);
    }

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

#[cfg(test)]
mod test_search_path {
    use crate::sem::{
        BaseParseContext, FullName, ParseContext, SchemaFileContext, SchemaName, SearchPathRead,
        TypeDefinition, TypeReference,
    };
    use std::path::Path;

    #[rstest::rstest]
    #[case::implicit_catalog_first(&["public", "app"], None, Some("pg_catalog"))]
    #[case::explicit_catalog_last(&["app", "pg_catalog"], None, Some("app"))]
    #[case::explicit_catalog_first(&["pg_catalog", "app"], None, Some("pg_catalog"))]
    #[case::configured_order(&["app", "public", "pg_catalog"], None, Some("app"))]
    #[case::qualified_outside_path(&["app"], Some("public"), Some("public"))]
    #[case::qualified_catalog(&["app"], Some("pg_catalog"), Some("pg_catalog"))]
    #[case::empty_path(&[], None, Some("pg_catalog"))]
    #[case::missing_qualified_name(&["public"], Some("missing"), None)]
    fn search_path_respects_qualification_and_catalog_precedence(
        #[case] search_path: &[&str],
        #[case] qualifier: Option<&str>,
        #[case] expected_schema: Option<&str>,
    ) {
        let mut context = SchemaFileContext::new(
            Path::new("schema.sql"),
            test_helpers::catalog_with_search_path(&["public"]),
        );
        context.set_search_path(&test_helpers::search_path(search_path));
        let name = if let Some(schema) = qualifier {
            FullName::with_schema(schema, "item")
        } else {
            FullName::from_local_name("item")
        };
        let expected_name = expected_schema.map(|schema| FullName::with_schema(schema, "item"));

        assert_eq!(
            context.get_type(&name).and_then(TypeDefinition::full_name),
            expected_name.as_ref()
        );
        let overloads = context.get_function_by_name(&name);
        let return_name = overloads
            .as_ref()
            .and_then(|overloads| overloads.get_at(0))
            .and_then(|overload| overload.ret_type().as_ref())
            .map(TypeReference::full_name);
        assert_eq!(return_name, expected_name.as_ref());
        assert_eq!(
            context.resolve_function_name_with_search_path(&name),
            expected_name.unwrap_or(name)
        );
    }

    #[test]
    fn nested_contexts_keep_their_own_search_paths() {
        let parent = SchemaFileContext::new(
            Path::new("parent.sql"),
            test_helpers::catalog_with_search_path(&["public", "pg_catalog"]),
        );
        let mut child = SchemaFileContext::new(Path::new("child.sql"), parent);
        assert_eq!(child.get_search_path(), child.base.get_search_path());
        child.set_search_path(&test_helpers::search_path(&["app", "pg_catalog"]));
        let mut descendant = SchemaFileContext::new(Path::new("descendant.sql"), child);
        assert_eq!(
            descendant.get_search_path(),
            descendant.base.get_search_path()
        );
        descendant.set_search_path(&[]);

        let name = FullName::from_local_name("item");
        assert!(descendant.get_search_path().is_empty());
        assert_eq!(descendant.get_default_schema(), None);
        assert_eq!(
            descendant
                .get_type(&name)
                .and_then(TypeDefinition::full_name),
            Some(&FullName::with_schema("pg_catalog", "item"))
        );
        assert_eq!(
            descendant.base.get_search_path(),
            test_helpers::search_path(&["app", "pg_catalog"])
        );
        assert_eq!(
            descendant.base.get_default_schema(),
            Some(SchemaName::from("app"))
        );
        assert_eq!(
            descendant
                .base
                .get_type(&name)
                .and_then(TypeDefinition::full_name),
            Some(&FullName::with_schema("app", "item"))
        );
        assert_eq!(
            descendant.base.base.get_search_path(),
            test_helpers::search_path(&["public", "pg_catalog"])
        );
        assert_eq!(
            descendant.base.base.get_default_schema(),
            Some(SchemaName::from("public"))
        );
        assert_eq!(
            descendant
                .base
                .base
                .get_type(&name)
                .and_then(TypeDefinition::full_name),
            Some(&FullName::with_schema("public", "item"))
        );
    }

    mod test_helpers {
        use crate::sem::{
            FullName, FunctionOverloadCollection, OverloadVariant, ParseContext, SchemaName,
            TypeDefinition, TypeReference,
        };
        use crate::test_helpers::TestParseContext;

        pub(super) fn search_path(schemas: &[&str]) -> Vec<SchemaName> {
            let mut names = Vec::new();
            for schema in schemas {
                names.push(SchemaName::from(*schema));
            }
            names
        }

        pub(super) fn catalog_with_search_path(schemas: &[&str]) -> TestParseContext {
            let mut context = TestParseContext::default();
            context.set_search_path(&search_path(schemas));
            for schema in ["pg_catalog", "public", "app"] {
                let name = FullName::with_schema(schema, "item");
                context.set_get_type_impl_result(&name, &TypeDefinition::builtin(&name));
                let overload = OverloadVariant::new(
                    &Some(TypeReference::from_full_name(name.clone())),
                    &Vec::new(),
                    true,
                    true,
                );
                context.set_get_function_impl_result(
                    &name,
                    &FunctionOverloadCollection::new(&[overload]),
                );
            }
            context
        }
    }
}
