mod cast_info;
#[cfg(test)]
mod creation_tests;
mod func_info;
#[cfg(test)]
mod name_identity_tests;
mod object_info;
mod ownership;
mod sequence_info;
mod table_info;
mod type_info;
mod view_info;

use crate::sem::{TableInfoRead, ViewInfoRead};
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
    cursors: crate::sem::CursorBindings,
    analysis_state: crate::sem::AnalysisState,
    parameter_constraints: crate::sem::ParameterConstraints,
}

#[derive(Debug)]
pub(super) struct SchemaCatalog {
    rules: std::collections::HashMap<crate::sem::RuleIdentity, Option<crate::sem::CreateRule>>,
    ownership: crate::sem::ownership::OwnershipCatalog,
    base: FrozenBaseContext,
    triggers:
        std::collections::HashMap<crate::sem::TriggerIdentity, Option<crate::sem::CreateTrigger>>,
    args: SqlFunArgs,
    metadata: ::sql_fun_core::SqlFunMetadata,
    search_path: Vec<SchemaName>,
    types: TypeDefinitionCollection,
    tables: TableDefinitionCollection,
    functions: FunctionDefinitionCollection,
    function_view_rows: std::collections::HashMap<(FullName, crate::sem::OverloadVariant), bool>,
    extensions: TrieMap<CreateExtension>,
    variables: TrieMap<SemScalarExpr>,
    schemas: TrieMap<CreateSchema>,
    sequences: SequenceInfoCollection,
    // Compound targets (signatures, columns, casts) use checked SQL target spelling.
    comments: TrieMap<(ObjectKind, String)>,
    views: ViewInformationCollection,
    // Concrete TypeReference spelling preserves canonical FullName + array identity.
    casts: TrieMap<TrieMap<CastDefinition>>,
    binary_operators: TrieMap<OperatorDefinition>,
    left_unary_operators: TrieMap<OperatorDefinition>,
    right_unary_operators: TrieMap<OperatorDefinition>,
    indexes: TrieMap<CreateIndex>,
    objects: TrieMap<crate::sem::Kind>,
    sql_source_path: PathBuf,
    source: Option<SourcePathWithSpan>,
}

impl SchemaFileContext {
    /// Create a context with explicit current-analysis configuration.
    ///
    /// The frozen base supplies catalog data only. Its retained configuration is
    /// historical evidence and is neither inherited nor reconciled with these inputs.
    ///
    /// The effective SQL search path continues the catalog layer chain (including
    /// prior SQL `SET` statements), independently of configuration ownership.
    /// The initial search path is copied from the supplied base. Later changes
    /// to this context's search path leave the base unchanged.
    /// A mutable builder cannot be supplied as the base, even by reference:
    ///
    /// ```compile_fail
    /// use std::path::Path;
    /// use ::sql_fun_sqlast::sem::SchemaFileContext;
    ///
    /// fn mutable_base(builder: SchemaFileContext, args: &::sql_fun_core::SqlFunArgs, metadata: &::sql_fun_core::SqlFunMetadata) {
    ///     let _next = SchemaFileContext::new(Path::new("schema.sql"), &builder, args, metadata);
    /// }
    /// ```
    pub fn new(
        sql_source_path: &Path,
        base: &FrozenBaseContext,
        args: &SqlFunArgs,
        metadata: &::sql_fun_core::SqlFunMetadata,
    ) -> Self {
        let search_path = base.get_search_path().to_vec();
        Self {
            catalog: SchemaCatalog {
                rules: std::collections::HashMap::new(),
                ownership: Default::default(),
                base: base.clone(),
                triggers: std::collections::HashMap::new(),
                args: args.clone(),
                metadata: metadata.clone(),
                search_path,
                sql_source_path: sql_source_path.to_path_buf(),
                types: TypeDefinitionCollection::default(),
                tables: TableDefinitionCollection::default(),
                functions: FunctionDefinitionCollection::default(),
                function_view_rows: std::collections::HashMap::default(),
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
                objects: TrieMap::default(),
                source: None,
            },
            statement_span: StringSpan::default(),
            source_access_service: None,
            reports: Default::default(),
            problems: Vec::new(),
            cursors: crate::sem::CursorBindings::default(),
            analysis_state: crate::sem::AnalysisState::default(),
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
        sem::ParseContext,
        test_helpers::{TestParseContext, test_context},
    };

    #[rstest::rstest]
    fn test_new(test_context: TestParseContext) {
        let path = Path::new("hoge.sql");
        let _context = crate::test_helpers::schema_layer(path, &test_context.freeze());
    }
}

impl IndexInfoRead for SchemaCatalog {
    fn indexes_for_table(&self, table: &crate::sem::TableName) -> Vec<&CreateIndex> {
        let mut indexes = self.base.indexes_for_table(table);
        indexes.retain(|index| !self.indexes.contains(&index.name().to_string()));
        indexes.extend(
            self.indexes
                .iter()
                .map(|(_, index)| index)
                .filter(|index| index.belongs_to(table)),
        );
        indexes
    }
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
        use std::path::Path;
        let parent = layer(&["b", "a"], &[index("a", "parent", "idx")])?.freeze();
        let mut child = crate::test_helpers::schema_layer(Path::new("child.sql"), &parent);
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
        // A rejected duplicate must continue reading the shared parent definition.
        assert_eq!(child.problems().len(), 1);
        assert!(std::ptr::eq(
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

    #[test]
    fn quoted_names_do_not_alias_or_shadow_other_indexes() -> testresult::TestResult {
        use crate::sem::{FullName, IndexInfoRead, ParseContext, TableName};
        let left = index("a.b", "parent_table", "c");
        let right = index("a", "other_table", "b.c");
        let parent = layer(&["a.b", "a"], std::slice::from_ref(&left))?.freeze();
        let child = crate::test_helpers::schema_layer(std::path::Path::new("child.sql"), &parent);
        let child = child.apply_create_index(&right)?;
        assert_eq!(
            child
                .get_index(&FullName::from_local_name("c"))
                .expect("search-path parent index")
                .name(),
            left.name()
        );
        assert_eq!(
            child
                .get_index(&FullName::from_local_name("b.c"))
                .expect("search-path child index")
                .name(),
            right.name()
        );
        assert_eq!(
            child
                .get_index_impl(left.name())
                .expect("parent index")
                .name(),
            left.name()
        );
        assert_eq!(
            child
                .get_index_impl(right.name())
                .expect("child index")
                .name(),
            right.name()
        );
        let table = TableName::from(FullName::with_schema("a.b", "parent_table"));
        assert_eq!(child.indexes_for_table(&table).len(), 1);
        let replacement = index("a.b", "replacement_table", "c");
        let frozen = child.apply_create_index(&replacement)?.freeze();
        assert_eq!(frozen.indexes_for_table(&table).len(), 1);
        assert_eq!(
            frozen
                .get_index_impl(right.name())
                .expect("other index")
                .name(),
            right.name()
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
            let mut context = crate::test_helpers::schema_layer(Path::new("indexes.sql"), &base);
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
            let mut parent = crate::test_helpers::schema_layer(Path::new("parent.sql"), &root);
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
            let mut child =
                crate::test_helpers::schema_layer(Path::new("child.sql"), &parent.freeze());
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
    use crate::sem::{AnalysisProblem, DiagnosticContext, FullName, ParseContext};
    use crate::test_helpers::TestParseContext;
    use std::path::Path;

    #[test]
    fn collects_problems_without_reporting_to_base() -> testresult::TestResult {
        let mut base = TestParseContext::default();
        base.reject_diagnostics_after(0);
        let base = base.freeze();
        let mut context = crate::test_helpers::schema_layer(Path::new("schema.sql"), &base);
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
        let descendant =
            crate::test_helpers::schema_layer(Path::new("descendant.sql"), parsed.base_context());
        assert!(descendant.problems().is_empty());
        let sibling = crate::test_helpers::schema_layer(Path::new("sibling.sql"), &base);
        assert!(sibling.problems().is_empty());
        Ok(())
    }
}

impl crate::sem::RuleInfoRead for SchemaCatalog {
    fn local_rule(
        &self,
        identity: &crate::sem::RuleIdentity,
    ) -> Option<Option<&crate::sem::CreateRule>> {
        self.rules.get(identity).map(Option::as_ref)
    }
    fn rule_parent(&self) -> Option<&dyn crate::sem::RuleInfoRead> {
        Some(&self.base)
    }
}
impl crate::sem::RuleInfoWrite for SchemaFileContext {
    fn store_rule(
        &mut self,
        identity: &crate::sem::RuleIdentity,
        definition: Option<&crate::sem::CreateRule>,
    ) {
        self.catalog
            .rules
            .insert(identity.clone(), definition.cloned());
    }
    fn store_rule_view(&mut self, target: &FullName, view: &crate::sem::ViewLike) {
        self.catalog.views.insert(target, view);
    }
}

impl BaseParseContext for SchemaCatalog {
    fn sqlite_relation_identity(&self, name: &FullName) -> Option<FullName> {
        if let Some(name) = self.tables.sqlite_name(name) {
            Some(name)
        } else if let Some(name) = self.views.sqlite_name(name) {
            Some(name)
        } else {
            self.base.sqlite_relation_identity(name)
        }
    }

    fn trigger_is_cancelled(&self, identity: &crate::sem::TriggerIdentity) -> bool {
        if let Some(entry) = self.triggers.get(identity) {
            entry.is_none()
        } else {
            self.base.trigger_is_cancelled(identity)
        }
    }
    fn trigger(
        &self,
        identity: &crate::sem::TriggerIdentity,
    ) -> Option<&crate::sem::CreateTrigger> {
        if let Some(trigger) = self.triggers.get(identity) {
            trigger.as_ref()
        } else {
            self.base.trigger(identity)
        }
    }
    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
        &self.metadata
    }

    fn get_args(&self) -> &SqlFunArgs {
        &self.args
    }

    fn get_source_file_path(&self) -> Option<&Path> {
        Some(self.sql_source_path.as_path())
    }
}

impl ParseContext for SchemaFileContext {
    fn apply_creation_group<F>(
        mut self,
        operation: F,
    ) -> Result<crate::AstAndContextPair<Self>, AnalysisError>
    where
        F: FnOnce(Self) -> Result<crate::AstAndContextPair<Self>, AnalysisError>,
    {
        // Stage a sparse child layer. Failed groups discard only their writes;
        // sharing the baseline avoids cloning catalog tries or definition bodies.
        let path = self.catalog.sql_source_path.clone();
        let args = self.catalog.args.clone();
        let metadata = self.catalog.metadata.clone();
        let empty = Self::new(&path, &self.catalog.base, &args, &metadata).catalog;
        let baseline = FrozenBaseContext::new(std::mem::replace(&mut self.catalog, empty));
        self.catalog = Self::new(&path, &baseline, &args, &metadata).catalog;
        self.catalog.source = baseline.get_source_path_with_span().cloned();
        let before = self.problems.len();
        let crate::AstAndContextPair(ast, mut context) = operation(self)?;
        if context.problems.len() != before {
            context.catalog = Self::new(&path, &baseline, &args, &metadata).catalog;
            context.catalog.source = baseline.get_source_path_with_span().cloned();
        }
        Ok(crate::AstAndContextPair::new(ast, context))
    }
    fn insert_role(&mut self, role: &crate::sem::Role) {
        self.catalog.ownership.insert_role(role);
    }
    fn insert_owned_object(&mut self, definition: &crate::sem::OwnedObjectDefinition) {
        self.catalog.ownership.insert(definition);
    }
    fn apply_alter_owner(self, alteration: &crate::sem::AlterOwner) -> Result<Self, AnalysisError> {
        self.apply_ownership_change(alteration)
    }

    fn register_trigger(
        mut self,
        trigger: &crate::sem::CreateTrigger,
    ) -> Result<Self, AnalysisError> {
        self.catalog
            .triggers
            .insert(trigger.identity().clone(), Some(trigger.clone()));
        Ok(self)
    }
    fn associate_trigger(
        mut self,
        target: &FullName,
        name: &FullName,
    ) -> Result<Self, AnalysisError> {
        if let Some(mut table) = self.get_table_impl(target).cloned() {
            table.associate_trigger(name);
            self.catalog.tables.insert_create_table(target, &table);
        } else if let Some(mut view) = self.get_view_impl(target).cloned() {
            view.associate_trigger(name);
            self.catalog.views.insert(target, &view);
        } else {
            self.report_analysis_problem(crate::sem::AnalysisProblem::trigger(
                &crate::sem::problem::TriggerProblem::target(target),
            ))?;
        }
        Ok(self)
    }

    fn cancel_trigger(
        mut self,
        identity: &crate::sem::TriggerIdentity,
    ) -> Result<Self, AnalysisError> {
        self.catalog.triggers.insert(identity.clone(), None);
        Ok(self)
    }
    fn dissociate_trigger(
        mut self,
        target: &FullName,
        name: &FullName,
    ) -> Result<Self, AnalysisError> {
        if let Some(mut table) = self.get_table_impl(target).cloned() {
            table.dissociate_trigger(name);
            self.catalog.tables.insert_create_table(target, &table);
        } else if let Some(mut view) = self.get_view_impl(target).cloned() {
            view.dissociate_trigger(name);
            self.catalog.views.insert(target, &view);
        } else {
            self.report_analysis_problem(crate::sem::AnalysisProblem::trigger(
                &crate::sem::problem::TriggerProblem::target(target),
            ))?;
        }
        Ok(self)
    }

    fn resolve_cursor(&self, name: &str) -> Option<&crate::sem::CursorInfo> {
        self.cursors.cursor(name)
    }
    fn bind_cursor(&mut self, name: &str, info: &crate::sem::CursorInfo) {
        self.cursors.bind(name, info);
    }
    fn remove_cursor(&mut self, name: &str) {
        self.cursors.remove(name);
    }
    fn analysis_state(&self) -> &crate::sem::AnalysisState {
        &self.analysis_state
    }
    fn analysis_state_mut(&mut self) -> &mut crate::sem::AnalysisState {
        &mut self.analysis_state
    }

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
        if crate::sem::ExtensionInfoRead::extension_exists(&self, name) {
            if !create_extension.if_not_exists() {
                self.report_problem(AnalysisProblem::creation(
                    crate::sem::problem::CreationProblem::named(
                        &FullName::from_local_name(name),
                        "extension",
                        &self.creation_span(),
                    ),
                ))?;
            }
            return Ok(self);
        }
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
        if crate::sem::OwnershipInfoRead::schema_definition(&self, name).is_some() {
            if !create_schema.if_not_exists() {
                self.report_problem(AnalysisProblem::creation(
                    crate::sem::problem::CreationProblem::named(
                        &FullName::from_local_name(name),
                        "schema",
                        &self.creation_span(),
                    ),
                ))?;
            }
            return Ok(self);
        }
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
        if !self.permits_relation_creation(
            name,
            crate::sem::Kind::index(),
            create_index.if_not_exists(),
            false,
        )? {
            return Ok(self);
        }
        self.catalog.store_index(create_index);
        Ok(self)
    }
}

#[cfg(test)]
mod test_extension_record {
    use crate::sem::{CreateExtension, ParseContext};
    use crate::test_helpers::TestParseContext;
    use std::path::Path;

    #[test]
    fn create_extension_records_the_name_without_loading_an_extension_file()
    -> testresult::TestResult {
        let root = TestParseContext::default().freeze();
        let extension = CreateExtension::from_name("not_installed_by_this_operation");
        let context = crate::test_helpers::schema_layer(Path::new("extension.sql"), &root)
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
        BaseParseContext, ColumnName, GetSourcePathWithSpanOpt, ParseContext, SchemaName,
        SearchPathRead, TableInfoWrite,
    };
    use std::path::Path;

    #[test]
    fn consuming_freeze_keeps_definitions_and_provenance_and_drops_source_service()
    -> testresult::TestResult {
        let base = test_helpers::empty_base();
        let mut context = crate::test_helpers::schema_layer(Path::new("parent.sql"), &base);
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
        let parent = crate::test_helpers::schema_layer(Path::new("parent.sql"), &base)
            .apply_create_table(&table)?
            .freeze();
        let mut left = crate::test_helpers::schema_layer(Path::new("left.sql"), &parent);
        let right = crate::test_helpers::schema_layer(Path::new("right.sql"), &parent);
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
    use crate::sem::{BaseParseContext, FullName, ParseContext, SchemaName, SearchPathRead};
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
        let mut context = crate::test_helpers::schema_layer(
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
            if let Some(ty) = context.get_type(&name) {
                ty.full_name()
            } else {
                None
            },
            expected_name.as_ref()
        );
        let overloads = context.get_function_by_name(&name);
        let return_name = if let Some(overloads) = overloads.as_ref()
            && let Some(overload) = overloads.get_at(0)
            && let Some(ret_type) = overload.ret_type().as_ref()
        {
            Some(ret_type.full_name())
        } else {
            None
        };
        assert_eq!(return_name, expected_name.as_ref());
        assert_eq!(
            context.resolve_function_name_with_search_path(&name),
            expected_name.unwrap_or(name)
        );
    }

    #[test]
    fn nested_contexts_keep_their_own_search_paths() {
        let parent = crate::test_helpers::schema_layer(
            Path::new("parent.sql"),
            &test_helpers::catalog_with_search_path(&["public", "pg_catalog"]),
        )
        .freeze();
        let mut child = crate::test_helpers::schema_layer(Path::new("child.sql"), &parent);
        assert_eq!(child.get_search_path(), parent.get_search_path());
        child.set_search_path(&test_helpers::search_path(&["app", "pg_catalog"]));
        let child = child.freeze();
        let mut descendant = crate::test_helpers::schema_layer(Path::new("descendant.sql"), &child);
        assert_eq!(descendant.get_search_path(), child.get_search_path());
        descendant.set_search_path(&[]);

        let name = FullName::from_local_name("item");
        assert!(descendant.get_search_path().is_empty());
        assert_eq!(descendant.get_default_schema(), None);
        assert_eq!(
            if let Some(ty) = descendant.get_type(&name) {
                ty.full_name()
            } else {
                None
            },
            Some(&FullName::with_schema("pg_catalog", "item"))
        );
        assert_eq!(
            child.get_search_path(),
            test_helpers::search_path(&["app", "pg_catalog"])
        );
        assert_eq!(child.get_default_schema(), Some(SchemaName::from("app")));
        assert_eq!(
            if let Some(ty) = child.get_type(&name) {
                ty.full_name()
            } else {
                None
            },
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
            if let Some(ty) = parent.get_type(&name) {
                ty.full_name()
            } else {
                None
            },
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

impl crate::sem::ConflictInfoRead for SchemaCatalog {
    fn database_collation(&self) -> Option<&crate::sem::DatabaseCollation> {
        self.base.database_collation()
    }
    fn collation_definition(
        &self,
        name: &crate::sem::FullName,
    ) -> Option<&crate::sem::CollationDefinition> {
        self.base.collation_definition(name)
    }
    fn operator_class_definition(
        &self,
        name: &crate::sem::FullName,
        method: &str,
    ) -> Option<&crate::sem::OperatorClassDefinition> {
        self.base.operator_class_definition(name, method)
    }
    fn lookup_sqlite_collation(
        &self,
        name: &crate::sem::FullName,
    ) -> crate::sem::CatalogNameLookup {
        self.base.lookup_sqlite_collation(name)
    }
    fn lookup_collation(&self, name: &FullName) -> crate::sem::CatalogNameLookup {
        crate::sem::conflict_masters::lookup_collation(self, name)
    }
    fn lookup_operator_class(
        &self,
        name: &FullName,
        access_method: &str,
    ) -> crate::sem::CatalogNameLookup {
        crate::sem::conflict_masters::lookup_operator_class(self, name, access_method)
    }
    fn infer_conflict_arbiters(
        &self,
        table: &crate::sem::TableName,
        conflict: &crate::sem::InsertConflict,
    ) -> Result<crate::sem::ConflictArbiterInference, AnalysisError> {
        crate::sem::parse_context::conflict_info::infer_from_catalog(self, table, conflict)
    }
}

impl crate::sem::ViewInsertInfoRead for SchemaCatalog {
    fn view_insert_support(&self, view: &FullName) -> Option<&crate::sem::ViewInsertSupport> {
        self.base.view_insert_support(view)
    }
}

impl crate::sem::ViewUpdateInfoRead for SchemaCatalog {
    fn view_update_support(
        &self,
        view: &crate::sem::FullName,
    ) -> Option<&crate::sem::ViewUpdateSupport> {
        self.base.view_update_support(view)
    }
}

impl crate::sem::ViewDeleteInfoRead for SchemaCatalog {
    fn view_delete_support(&self, _view: &crate::sem::FullName) -> crate::sem::ViewDeleteSupport {
        todo!("catalog deletable views, INSTEAD OF DELETE triggers and rules")
    }
}

impl crate::sem::ViewMergeInfoRead for SchemaCatalog {
    fn view_merge_support(&self, _view: &crate::sem::FullName) -> crate::sem::ViewMergeSupport {
        todo!("catalog MERGE-eligible views, rules, triggers and action columns")
    }
}
