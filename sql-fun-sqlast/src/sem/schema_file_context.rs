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
        CreateIndex, CreateSchema, DiagnosticContext, FrozenBaseContext, FullName,
        GetSourcePathWithSpanOpt, ImplicitChange, MutSourcePathWithSpan, ObjectKind,
        OperatorDefinition, OperatorInfoRead, OperatorName, ParseContext, SchemaName,
        SemScalarExpr, SourceAccessService, VariableSet,
        parse_context::{IndexInfoRead, SearchPathRead},
    },
};

use super::SourcePathWithSpan;

/// Mutable schema layer built over an immutable base.
///
/// Definitions and search-path changes belong to this layer. Consuming it with
/// [`ParseContext::freeze`] shares the completed catalog and discards diagnostic
/// rendering state. Schema-file loaders retain problems on their parsed result.
#[derive(Debug)]
pub struct SchemaFileContext {
    catalog: SchemaCatalog,
    statement_span: StringSpan,
    source_access_service: Option<Box<dyn SourceAccessService>>,
    reports: Vec<String>,
    problems: Vec<AnalysisProblem>,
    parameter_constraints: crate::sem::ParameterConstraints,
}

#[derive(Debug)]
pub(super) struct SchemaCatalog {
    base: FrozenBaseContext,
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
    sql_source_path: PathBuf,
    source: Option<SourcePathWithSpan>,
}

impl SchemaFileContext {
    /// create context for loading schema file
    ///
    /// The initial search path is copied from the supplied base. Later changes
    /// to this context's search path leave the base unchanged.
    /// A mutable builder cannot be supplied as the base, even by reference:
    ///
    /// ```compile_fail
    /// use std::path::Path;
    /// use ::sql_fun_sqlast::sem::SchemaFileContext;
    ///
    /// fn mutable_base(builder: SchemaFileContext) {
    ///     let _next = SchemaFileContext::new(Path::new("schema.sql"), &builder);
    /// }
    /// ```
    pub fn new(sql_source_path: &Path, base: &FrozenBaseContext) -> Self {
        let search_path = base.get_search_path().to_vec();
        Self {
            catalog: SchemaCatalog {
                base: base.clone(),
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
                indexes: TrieMap::default(),
                source: None,
            },
            statement_span: StringSpan::default(),
            source_access_service: None,
            reports: Default::default(),
            problems: Vec::new(),
            parameter_constraints: crate::sem::ParameterConstraints::default(),
        }
    }

    /// Recoverable problems collected by this layer, in reporting order.
    ///
    /// A diagnostic rendering failure is returned immediately and the rejected
    /// problem is not collected. Earlier accepted problems remain available.
    pub fn problems(&self) -> &[AnalysisProblem] {
        &self.problems
    }

    pub(super) const fn catalog(&self) -> &SchemaCatalog {
        &self.catalog
    }

    pub(crate) fn into_parsed(
        mut self,
        sem_results: Vec<crate::sem::SemAst>,
    ) -> crate::ParsedSchemaDump {
        let problems = std::mem::take(&mut self.problems);
        crate::ParsedSchemaDump::new(sem_results, self.freeze(), problems)
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

impl GetSourcePathWithSpanOpt for SchemaCatalog {
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }
}

impl MutSourcePathWithSpan for SchemaFileContext {
    fn set_source_path_with_span(&mut self, value: &SourcePathWithSpan) {
        self.catalog.source = Some(value.clone())
    }
}

#[cfg(test)]
mod test_schema_file_context {
    use std::path::Path;

    use crate::{
        sem::{ParseContext, SchemaFileContext},
        test_helpers::{TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_new(test_context: TestParseContext) {
        let path = Path::new("hoge.sql");
        let _context = SchemaFileContext::new(path, &test_context.freeze());
    }
}

impl IndexInfoRead for SchemaCatalog {
    fn get_index_impl(&self, index_name: &FullName) -> Option<&CreateIndex> {
        if let Some(index) = self.indexes.get(&index_name.to_string()) {
            Some(index)
        } else {
            self.base.get_index_impl(index_name)
        }
    }
}

#[cfg(test)]
mod test_index_lookup {
    use self::test_helpers::{index, layer};
    use crate::sem::{BaseParseContext, FullName, IndexInfoRead, ParseContext};

    #[rstest::rstest]
    #[case::first(&["a", "b"], None, Some("a"))]
    #[case::reversed(&["b", "a"], None, Some("b"))]
    #[case::qualified(&["b"], Some("a"), Some("a"))]
    #[case::empty(&[], None, None)]
    #[case::missing(&["a", "b"], Some("missing"), None)]
    fn lookup_uses_identity_and_search_path(
        #[case] path: &[&str],
        #[case] qualifier: Option<&str>,
        #[case] expected: Option<&str>,
    ) -> testresult::TestResult {
        let context = layer(
            path,
            &[index("a", "items", "idx"), index("b", "items", "idx")],
        )?;
        let name = qualifier.map_or_else(
            || FullName::from_local_name("idx"),
            |schema| FullName::with_schema(schema, "idx"),
        );
        let expected = expected.map(|schema| FullName::with_schema(schema, "idx"));
        assert_eq!(
            context.get_index(&name).map(|index| index.name()),
            expected.as_ref()
        );
        assert!(
            context
                .get_index_impl(&FullName::from_local_name("idx"))
                .is_none()
        );
        Ok(())
    }

    #[rstest::rstest]
    #[case::implicit(&["a"], "pg_catalog")]
    #[case::explicit_last(&["a", "pg_catalog"], "a")]
    fn index_lookup_reuses_catalog_precedence(
        #[case] path: &[&str],
        #[case] expected: &str,
    ) -> testresult::TestResult {
        let context = layer(
            path,
            &[
                index("pg_catalog", "system", "idx"),
                index("a", "items", "idx"),
            ],
        )?;
        assert_eq!(
            context
                .get_index(&FullName::from_local_name("idx"))
                .expect("index")
                .name(),
            &FullName::with_schema(expected, "idx")
        );
        Ok(())
    }

    #[test]
    fn frozen_parent_is_searched_per_qualified_candidate() -> testresult::TestResult {
        use crate::sem::SchemaFileContext;
        use std::path::Path;
        let parent = layer(&["b", "a"], &[index("a", "parent", "idx")])?.freeze();
        let mut child = SchemaFileContext::new(Path::new("child.sql"), &parent);
        child.set_search_path(&[
            crate::sem::SchemaName::from("a"),
            crate::sem::SchemaName::from("b"),
        ]);
        child = child.apply_create_index(&index("b", "child", "idx"))?;
        assert_eq!(
            child
                .get_index(&FullName::from_local_name("idx"))
                .unwrap()
                .name(),
            &FullName::with_schema("a", "idx")
        );
        let local = index("a", "replacement", "idx");
        child = child.apply_create_index(&local)?;
        // Compare object addresses to verify local precedence for the same identity.
        assert!(!std::ptr::eq(
            child.get_index(local.name()).unwrap(),
            parent.get_index_impl(local.name()).unwrap()
        ));
        assert!(
            child
                .get_index(&FullName::from_local_name("absent"))
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn sqlite_qualified_catalog_identities_coexist() -> testresult::TestResult {
        let context = layer(
            &["main", "aux"],
            &[index("main", "items", "idx"), index("aux", "items", "idx")],
        )?;
        assert!(
            context
                .get_index_impl(&FullName::with_schema("main", "idx"))
                .is_some()
        );
        assert!(
            context
                .get_index_impl(&FullName::with_schema("aux", "idx"))
                .is_some()
        );
        Ok(())
    }

    mod test_helpers {
        use crate::sem::{
            CreateIndex, FullName, IndexElementCollection, ParseContext, SchemaFileContext,
            SchemaName, TableName,
        };
        use crate::test_helpers::TestParseContext;
        use std::path::Path;

        pub(super) fn index(schema: &str, table: &str, name: &str) -> CreateIndex {
            CreateIndex::new(
                &FullName::with_schema(schema, name),
                &TableName::from(FullName::with_schema(schema, table)),
                &IndexElementCollection::default(),
            )
        }

        pub(super) fn layer(
            path: &[&str],
            indexes: &[CreateIndex],
        ) -> Result<SchemaFileContext, crate::sem::AnalysisError> {
            let base = TestParseContext::default().freeze();
            let mut context = SchemaFileContext::new(Path::new("indexes.sql"), &base);
            let mut schemas = Vec::new();
            for name in path {
                schemas.push(SchemaName::from(name));
            }
            context.set_search_path(&schemas);
            for index in indexes {
                context = context.apply_create_index(index)?;
            }
            Ok(context)
        }
    }
}

impl OperatorInfoRead for SchemaCatalog {
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

#[cfg(test)]
mod test_operator_layers {
    use crate::sem::{OperatorInfoRead, OperatorName, PgBuiltInType};

    #[rstest::rstest]
    #[case::binary(true, true)]
    #[case::left_unary(true, false)]
    #[case::right_unary(false, true)]
    fn local_operator_shadows_parent_while_other_names_fall_back(
        #[case] has_left: bool,
        #[case] has_right: bool,
    ) {
        let child = test_helpers::operator_layer(has_left, has_right);
        let local = child
            .get_operator_op(&OperatorName::new("+"), has_left, has_right)
            .expect("local operator");
        test_helpers::assert_returns(local, &PgBuiltInType::int8());
        let inherited = child
            .get_operator_op(&OperatorName::new("-"), has_left, has_right)
            .expect("inherited operator");
        test_helpers::assert_returns(inherited, &PgBuiltInType::int4());
    }

    mod test_helpers {
        use crate::sem::{
            BinaryOperatorDefinition, OperatorDefinition, ParseContext, PgBuiltInType,
            SchemaFileContext, TypeReference, UnaryOperatorDefinition,
        };
        use crate::test_helpers::TestParseContext;
        use std::path::Path;

        pub(super) fn operator_layer(
            has_left: bool,
            has_right: bool,
        ) -> crate::sem::FrozenBaseContext {
            let root = TestParseContext::default().freeze();
            let mut parent = SchemaFileContext::new(Path::new("parent.sql"), &root);
            register(
                &mut parent,
                "+",
                has_left,
                has_right,
                &PgBuiltInType::int4(),
            );
            register(
                &mut parent,
                "-",
                has_left,
                has_right,
                &PgBuiltInType::int4(),
            );
            let mut child = SchemaFileContext::new(Path::new("child.sql"), &parent.freeze());
            register(&mut child, "+", has_left, has_right, &PgBuiltInType::int8());
            child.freeze()
        }

        fn register(
            context: &mut SchemaFileContext,
            name: &str,
            left: bool,
            right: bool,
            result: &TypeReference,
        ) {
            let argument = PgBuiltInType::int4();
            if left && right {
                let overload = BinaryOperatorDefinition::new(&argument, &argument, result, true);
                context
                    .catalog
                    .binary_operators
                    .insert(name, OperatorDefinition::binary(&[overload]));
            } else if left {
                let overload = UnaryOperatorDefinition::new(&argument, result, true);
                context
                    .catalog
                    .left_unary_operators
                    .insert(name, OperatorDefinition::left_unary(&[overload]));
            } else {
                let overload = UnaryOperatorDefinition::new(&argument, result, true);
                context
                    .catalog
                    .right_unary_operators
                    .insert(name, OperatorDefinition::right_unary(&[overload]));
            }
        }

        pub(super) fn assert_returns(operator: &OperatorDefinition, expected: &TypeReference) {
            match operator {
                OperatorDefinition::Binary(overloads) => {
                    assert_eq!(overloads.len(), 1);
                    assert_eq!(overloads[0].get_return_type(), expected);
                }
                OperatorDefinition::LeftUnary(overloads)
                | OperatorDefinition::RightUnary(overloads) => {
                    assert_eq!(overloads.len(), 1);
                    assert_eq!(overloads[0].get_return_type(), expected);
                }
            }
        }
    }
}

impl SearchPathRead for SchemaCatalog {
    fn get_default_schema(&self) -> Option<SchemaName> {
        self.search_path.first().cloned()
    }

    fn get_search_path(&self) -> &[SchemaName] {
        &self.search_path
    }
}

impl DiagnosticContext for SchemaFileContext {
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        if let Some(source_provider) = &self.source_access_service {
            let source = source_provider.get_source_span(
                self.get_args(),
                self.catalog.sql_source_path.as_path(),
                self.statement_span,
            )?;
            let theme = self.get_args().highlighter_theme()?;

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
    use crate::sem::{
        AnalysisProblem, DiagnosticContext, FullName, ParseContext, SchemaFileContext,
    };
    use crate::test_helpers::TestParseContext;
    use std::path::Path;

    #[test]
    fn collects_problems_without_reporting_to_base() -> testresult::TestResult {
        let mut base = TestParseContext::default();
        base.reject_diagnostics_after(0);
        let base = base.freeze();
        let mut context = SchemaFileContext::new(Path::new("schema.sql"), &base);
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

        assert_eq!(context.problems().len(), 2);
        let parsed = context.into_parsed(Vec::new());
        assert_eq!(parsed.problems().len(), 2);
        let descendant = SchemaFileContext::new(Path::new("descendant.sql"), parsed.base_context());
        assert!(descendant.problems().is_empty());
        let sibling = SchemaFileContext::new(Path::new("sibling.sql"), &base);
        assert!(sibling.problems().is_empty());
        Ok(())
    }
}

impl BaseParseContext for SchemaCatalog {
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

impl ParseContext for SchemaFileContext {
    fn parameter_constraints(&self) -> &crate::sem::ParameterConstraints {
        &self.parameter_constraints
    }

    fn set_parameter_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        self.parameter_constraints = constraints.clone();
    }

    fn freeze(self) -> FrozenBaseContext {
        FrozenBaseContext::new(self.catalog)
    }

    fn set_search_path(&mut self, search_path: &[SchemaName]) {
        self.catalog.search_path = search_path.to_vec();
    }

    fn set_statement_span(&mut self, statement_span: StringSpan) {
        self.statement_span = statement_span;
        self.catalog.source = Some(SourcePathWithSpan::new(
            &self.catalog.sql_source_path,
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
        self.catalog
            .extensions
            .insert(name, create_extension.clone());
        Ok(self)
    }

    fn apply_variable_set(mut self, variable_set: &VariableSet) -> Result<Self, AnalysisError> {
        let name = variable_set.name();
        self.catalog
            .variables
            .insert(name, variable_set.value().clone());
        Ok(self)
    }

    fn apply_create_schema(mut self, create_schema: &CreateSchema) -> Result<Self, AnalysisError> {
        let name = create_schema.name();
        self.catalog.schemas.insert(name, create_schema.clone());
        Ok(self)
    }

    fn apply_comment(mut self, comment: &Comment) -> Result<Self, AnalysisError> {
        let target = comment.target();
        self.catalog.comments.insert(
            target,
            (*comment.object_type(), String::from(comment.comment())),
        );
        Ok(self)
    }

    fn apply_create_index(mut self, create_index: &CreateIndex) -> Result<Self, AnalysisError> {
        let name = create_index.name();
        self.catalog
            .indexes
            .insert(&name.to_string(), create_index.clone());
        Ok(self)
    }
}

#[cfg(test)]
mod test_extension_record {
    use crate::sem::{CreateExtension, ParseContext, SchemaFileContext};
    use crate::test_helpers::TestParseContext;
    use std::path::Path;

    #[test]
    fn create_extension_records_the_name_without_loading_an_extension_file()
    -> testresult::TestResult {
        let root = TestParseContext::default().freeze();
        let extension = CreateExtension::from_name("not_installed_by_this_operation");
        let context = SchemaFileContext::new(Path::new("extension.sql"), &root)
            .apply_create_extension(&extension)?;

        assert_eq!(
            context
                .catalog
                .extensions
                .get(extension.name())
                .expect("local extension record")
                .name(),
            extension.name()
        );
        assert!(context.problems().is_empty());
        Ok(())
    }
}

#[cfg(test)]
mod test_freeze {
    use crate::StringSpan;
    use crate::sem::{
        BaseParseContext, ColumnName, GetSourcePathWithSpanOpt, ParseContext, SchemaFileContext,
        SchemaName, SearchPathRead, TableInfoWrite,
    };
    use std::path::Path;

    #[test]
    fn consuming_freeze_keeps_definitions_and_provenance_and_drops_source_service()
    -> testresult::TestResult {
        let base = test_helpers::empty_base();
        let mut context = SchemaFileContext::new(Path::new("parent.sql"), &base);
        let statement_span = StringSpan::new(12, 24);
        context.set_statement_span(statement_span);
        let table = test_helpers::table_with_id();
        context = context.apply_create_table(&table)?;
        let source_lifetime = test_helpers::attach_local_source(&mut context);

        let frozen = context.freeze();

        assert!(source_lifetime.upgrade().is_none());
        assert!(
            frozen
                .get_table(table.name())
                .expect("frozen table")
                .has_column(&ColumnName::from("id"))
        );
        assert!(base.get_table(table.name()).is_none());
        assert_eq!(frozen.get_source_file_path(), Some(Path::new("parent.sql")));
        let source = frozen.get_source_path_with_span().expect("last provenance");
        assert_eq!(source.source_path(), Path::new("parent.sql"));
        assert_eq!(source.span().seek_pos(), statement_span.seek_pos());
        assert_eq!(source.span().len(), statement_span.len());
        Ok(())
    }

    #[test]
    fn child_alteration_leaves_shared_parent_and_sibling_unchanged() -> testresult::TestResult {
        let base = test_helpers::empty_base();
        let table = test_helpers::table_with_key_candidates();
        let parent = SchemaFileContext::new(Path::new("parent.sql"), &base)
            .apply_create_table(&table)?
            .freeze();
        let mut left = SchemaFileContext::new(Path::new("left.sql"), &parent);
        let right = SchemaFileContext::new(Path::new("right.sql"), &parent);
        left.set_search_path(&[SchemaName::from("left")]);
        let left = left
            .apply_alter_table(&test_helpers::add_primary_key(&table, "other_id"))?
            .freeze();
        let right = right.freeze();

        let left_column = ColumnName::from("other_id");
        let left_table = left.get_table(table.name()).expect("left table");
        let right_table = right.get_table(table.name()).expect("right table");
        let parent_table = parent.get_table(table.name()).expect("parent table");
        assert_eq!(left_table.primary_key_columns(), vec![&left_column]);
        assert!(right_table.primary_key_columns().is_empty());
        assert!(parent_table.primary_key_columns().is_empty());
        assert_eq!(left.get_default_schema(), Some(SchemaName::from("left")));
        assert_eq!(right.get_default_schema(), Some(SchemaName::from("public")));
        assert_eq!(
            parent.get_default_schema(),
            Some(SchemaName::from("public"))
        );
        Ok(())
    }

    mod test_helpers {
        use crate::sem::{
            AlterObjSubCommand, AlterTable, ColumnDefinition, ColumnName, Constraint, CreateTable,
            CreateTableMode, FrozenBaseContext, FullName, ParseContext, PgBuiltInType,
            PrimaryKeyConstraint, SchemaFileContext, SchemaName, SourceAccessService, TableName,
        };
        use crate::test_helpers::{TestParseContext, source_span};
        use std::rc::{Rc, Weak};

        pub(super) fn empty_base() -> FrozenBaseContext {
            let mut context = TestParseContext::default();
            context.set_search_path(&[SchemaName::from("public")]);
            context.freeze()
        }

        fn column(name: &str) -> ColumnDefinition {
            ColumnDefinition::new(
                &Some(ColumnName::from(name)),
                Some(&PgBuiltInType::int4()),
                Some(true),
                &[],
            )
        }

        pub(super) fn table_with_id() -> CreateTable {
            CreateTable::new(
                &TableName::from(FullName::with_schema("public", "items")),
                vec![column("id")],
                CreateTableMode::from_if_not_exists(false),
                false,
                &source_span(),
            )
        }

        pub(super) fn table_with_key_candidates() -> CreateTable {
            CreateTable::new(
                &TableName::from(FullName::with_schema("public", "items")),
                vec![column("id"), column("other_id")],
                CreateTableMode::from_if_not_exists(false),
                false,
                &source_span(),
            )
        }

        pub(super) fn add_primary_key(table: &CreateTable, name: &str) -> AlterTable {
            let primary_key =
                PrimaryKeyConstraint::new("items_pk", table.name(), &[ColumnName::from(name)]);
            let constraint = Constraint::primary_key(&primary_key);
            AlterTable::new(
                table.name(),
                &[AlterObjSubCommand::add_constraint(&constraint)],
                &source_span(),
            )
        }

        #[derive(Debug)]
        struct LocalSource {
            _lifetime: Rc<()>,
        }

        impl SourceAccessService for LocalSource {}

        pub(super) fn attach_local_source(context: &mut SchemaFileContext) -> Weak<()> {
            let lifetime = Rc::new(());
            let observer = Rc::downgrade(&lifetime);
            context.set_source_file(Box::new(LocalSource {
                _lifetime: lifetime,
            }));
            observer
        }
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
            &test_helpers::catalog_with_search_path(&["public"]),
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
            &test_helpers::catalog_with_search_path(&["public", "pg_catalog"]),
        )
        .freeze();
        let mut child = SchemaFileContext::new(Path::new("child.sql"), &parent);
        assert_eq!(child.get_search_path(), parent.get_search_path());
        child.set_search_path(&test_helpers::search_path(&["app", "pg_catalog"]));
        let child = child.freeze();
        let mut descendant = SchemaFileContext::new(Path::new("descendant.sql"), &child);
        assert_eq!(descendant.get_search_path(), child.get_search_path());
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
            child.get_search_path(),
            test_helpers::search_path(&["app", "pg_catalog"])
        );
        assert_eq!(child.get_default_schema(), Some(SchemaName::from("app")));
        assert_eq!(
            child.get_type(&name).and_then(TypeDefinition::full_name),
            Some(&FullName::with_schema("app", "item"))
        );
        assert_eq!(
            parent.get_search_path(),
            test_helpers::search_path(&["public", "pg_catalog"])
        );
        assert_eq!(
            parent.get_default_schema(),
            Some(SchemaName::from("public"))
        );
        assert_eq!(
            parent.get_type(&name).and_then(TypeDefinition::full_name),
            Some(&FullName::with_schema("public", "item"))
        );
    }

    mod test_helpers {
        use crate::sem::{
            FrozenBaseContext, FullName, FunctionOverloadCollection, OverloadVariant, ParseContext,
            SchemaName, TypeDefinition, TypeReference,
        };
        use crate::test_helpers::TestParseContext;

        pub(super) fn search_path(schemas: &[&str]) -> Vec<SchemaName> {
            let mut names = Vec::new();
            for schema in schemas {
                names.push(SchemaName::from(schema));
            }
            names
        }

        pub(super) fn catalog_with_search_path(schemas: &[&str]) -> FrozenBaseContext {
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
            context.freeze()
        }
    }
}
