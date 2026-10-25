mod cast_info;
mod conflict_info;

use crate::{
    StringSpan,
    context::ParseContext,
    sem::{
        AlterSequence, AlterTable, AlterView, AnalysisError, AnalysisProblem, BaseParseContext,
        BinaryOperatorDefinition, CastContext, CastDefinition, ColumnDefinition, Comment,
        CreateCompositeType, CreateDomain, CreateEnum, CreateExtension, CreateFunction,
        CreateIndex, CreateSchema, CreateSequence, CreateTable, CreateTableAs, CreateTableMode,
        CreateView, DiagnosticContext, DropTable, FromClause, FullName, FunctionInfoRead,
        FunctionInfoWrite, FunctionOverloadCollection, GetSourcePathWithSpanOpt, IndexInfoRead,
        MutSourcePathWithSpan, OperatorDefinition, OperatorInfoRead, OperatorName, OverloadVariant,
        SchemaName, SearchPathRead, SequenceInfoRead, SequenceInfoWrite, SourcePathWithSpan,
        TableInfoRead, TableInfoWrite, TableName, TypeDefinition, TypeInfoRead, TypeInfoWrite,
        TypeReference, ValidOperatorName, VariableSet, ViewInfoRead, ViewInfoWrite, ViewLike,
        WithClause,
    },
    test_helpers::{SynBuilder, source_span},
};

use std::collections::{HashMap, HashSet};

/// ParseContext implementation for tests
#[derive(Debug, Default)]
pub struct TestParseContext {
    rules: HashMap<crate::sem::RuleIdentity, Option<crate::sem::CreateRule>>,
    ownership: crate::sem::ownership::OwnershipCatalog,
    triggers: HashMap<crate::sem::TriggerIdentity, Option<crate::sem::CreateTrigger>>,
    masters: Option<crate::sem::ConflictMasters>,
    collation_results: HashMap<FullName, crate::sem::CatalogNameLookup>,
    operator_class_results: HashMap<(FullName, String), crate::sem::CatalogNameLookup>,
    expected_conflict_target: Option<TableName>,
    expected_conflict_modifiers: Option<Vec<crate::sem::InsertInferenceModifiers>>,
    conflict_arbiters: Vec<crate::sem::InsertArbiter>,
    metadata: Option<::sql_fun_core::SqlFunMetadata>,
    implicit_cast_results: HashMap<TypeReference, HashMap<TypeReference, Option<CastDefinition>>>,
    implicit_castable_results: HashMap<TypeReference, HashSet<TypeReference>>,
    operator_op_results: HashMap<(OperatorName, bool, bool), Option<OperatorDefinition>>,
    search_path: Vec<SchemaName>,
    type_definitions: HashMap<FullName, TypeDefinition>,
    array_elements: HashMap<TypeReference, TypeReference>,
    table_definitions: HashMap<FullName, Option<CreateTable>>,
    view_definitions: HashMap<FullName, Option<ViewLike>>,
    view_delete_support: HashMap<FullName, crate::sem::ViewDeleteSupport>,
    view_update_support: HashMap<FullName, crate::sem::ViewUpdateSupport>,
    view_insert_support: HashMap<FullName, crate::sem::ViewInsertSupport>,
    view_merge_support: HashMap<FullName, crate::sem::ViewMergeSupport>,
    function_definitions: HashMap<FullName, FunctionOverloadCollection>,
    aggregate_functions: HashSet<FullName>,
    sequence_definitions: HashMap<FullName, CreateSequence>,
    function_row_properties: HashMap<(FullName, OverloadVariant), bool>,
    alter_tables: Vec<AlterTable>,
    drop_tables: Vec<DropTable>,
    create_indexes: Vec<CreateIndex>,
    problems: Vec<AnalysisProblem>,
    cursors: crate::sem::CursorBindings,
    analysis_state: crate::sem::AnalysisState,
    parameter_constraints: crate::sem::ParameterConstraints,
    parameter_type_hints: crate::sem::ParameterTypeHints,
    report_failure_after: Option<usize>,
    source: Option<SourcePathWithSpan>,
    table_ref_types: Vec<TableName>,
}

/// get [TestParseContext]
#[rstest::fixture]
pub fn test_context() -> TestParseContext {
    TestParseContext::default()
}

impl TestParseContext {
    /// Select this fixture's analysis environment explicitly.
    pub(crate) fn with_dialect(mut self, dialect: crate::ast::Dialect) -> Self {
        let selected = if dialect == crate::ast::Dialect::sqlite() {
            "SQLite"
        } else {
            "postgres"
        };
        self.metadata = Some(
            ::serde_json::from_value(::serde_json::json!({
                "engine": selected, "dialect": selected, "version": "17", "search_path": ["public"]
            }))
            .expect("explicit fixture environment"),
        );
        self
    }

    pub(crate) fn with_metadata(mut self, metadata: &::sql_fun_core::SqlFunMetadata) -> Self {
        self.metadata = Some(metadata.clone());
        self
    }

    pub(crate) fn reject_diagnostics_after(&mut self, count: usize) {
        self.report_failure_after = Some(count);
    }

    pub(crate) fn has_reported_problem(&self, code: &str) -> bool {
        self.problems.iter().any(|problem| problem.code() == code)
    }
    pub(crate) fn reported_problems(&self) -> &[AnalysisProblem] {
        &self.problems
    }
    /// setup [SearchPathRead::get_search_path] result
    pub fn set_get_search_path_result(&mut self, search_path: &[SchemaName]) {
        self.search_path = search_path.to_owned()
    }

    /// setup [TypeInfoRead::get_type_impl] result
    pub fn set_get_type_impl_result(
        &mut self,
        type_name: &FullName,
        type_definition: &TypeDefinition,
    ) {
        self.type_definitions
            .insert(type_name.clone(), type_definition.clone());
    }

    pub(crate) fn set_array_element(&mut self, array: &TypeReference, element: &TypeReference) {
        self.array_elements.insert(array.clone(), element.clone());
    }

    /// setup type for TypeReference
    pub fn setup_type(&mut self, type_ref: TypeReference) {
        let type_name = type_ref.full_name();
        let type_definition = TypeDefinition::builtin(type_name);
        self.set_get_type_impl_result(type_name, &type_definition);

        let cast_definition = Some(CastDefinition::new(CastContext::NoConversion));
        self.set_get_implicit_cast_result(&type_ref, &type_ref, cast_definition);
    }
}

impl GetSourcePathWithSpanOpt for TestParseContext {
    fn get_source_path_with_span(&self) -> Option<&SourcePathWithSpan> {
        self.source.as_ref()
    }
}

impl MutSourcePathWithSpan for TestParseContext {
    fn set_source_path_with_span(&mut self, value: &SourcePathWithSpan) {
        self.source = Some(value.clone())
    }
}

impl SearchPathRead for TestParseContext {
    fn get_search_path(&self) -> &[SchemaName] {
        self.search_path.as_slice()
    }

    fn get_default_schema(&self) -> Option<SchemaName> {
        self.search_path.first().cloned()
    }
}

impl TestParseContext {
    /// setup [ViewInfoRead::get_view_impl] result
    pub fn set_get_view_impl_result(
        &mut self,
        view_name: &FullName,
        view_definition: &Option<ViewLike>,
    ) {
        self.view_definitions
            .insert(view_name.clone(), view_definition.clone());
    }
}

impl ViewInfoRead for TestParseContext {
    fn get_view_impl(&self, key: &FullName) -> Option<&ViewLike> {
        if let Some(v) = self.view_definitions.get(key) {
            v.as_ref()
        } else {
            None
        }
    }
}

impl ViewInfoWrite for TestParseContext {
    fn apply_alter_view(self, _alter_table: &AlterView) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_create_view(self, _create_view: &CreateView) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_create_table_as(
        self,
        _create_table_as: &CreateTableAs,
    ) -> Result<Self, AnalysisError> {
        todo!()
    }
}

impl TestParseContext {
    /// setup [TableInfoRead::get_table_impl] result
    pub fn set_get_table_impl_result(
        &mut self,
        table_name: &FullName,
        table_definition: &Option<CreateTable>,
    ) {
        self.table_definitions
            .insert(table_name.clone(), table_definition.clone());
    }

    /// setup [TableInfoRead::get_table_impl] result
    pub fn setup_table(
        &mut self,
        table_name: &str,
        columns: &[ColumnDefinition],
        source_span: &SourcePathWithSpan,
    ) {
        let table_full_name = FullName::with_schema("public", table_name);
        let table_definition = CreateTable::new(
            &TableName::from(table_full_name.clone()),
            columns.to_vec(),
            CreateTableMode::Default,
            false,
            source_span,
        );
        self.set_get_table_impl_result(&table_full_name, &Some(table_definition));
    }

    /// get alter table command length
    pub fn alter_tables_len(&self) -> usize {
        self.alter_tables.len()
    }

    /// get applied create index length
    pub fn create_indexes_len(&self) -> usize {
        self.create_indexes.len()
    }

    /// get last applied create index
    pub fn last_create_index(&self) -> Option<&CreateIndex> {
        self.create_indexes.last()
    }
}

impl TableInfoRead for TestParseContext {
    fn get_table_impl(&self, key: &FullName) -> Option<&CreateTable> {
        if let Some(v) = self.table_definitions.get(key) {
            v.as_ref()
        } else {
            None
        }
    }
}

impl TableInfoWrite for TestParseContext {
    fn apply_alter_table(mut self, alter_table: &AlterTable) -> Result<Self, AnalysisError> {
        self.alter_tables.push(alter_table.clone());
        Ok(self)
    }

    fn apply_drop_table(mut self, drop_table: &DropTable) -> Result<Self, AnalysisError> {
        self.drop_tables.push(drop_table.clone());
        Ok(self)
    }

    fn insert_create_table(&mut self, table_name: &FullName, create_table: &CreateTable) {
        self.table_definitions
            .insert(table_name.clone(), Some(create_table.clone()));
    }
}

impl TypeInfoRead for TestParseContext {
    fn type_references(&self) -> Vec<TypeReference> {
        self.type_definitions
            .keys()
            .map(|name| TypeReference::concrete_type_ref(name, false))
            .collect()
    }

    fn builtin_type_ordered(&self, _name: &FullName) -> Option<bool> {
        None
    }

    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.type_definitions.get(key)
    }

    fn get_array_item_type(&self, type_name: &TypeReference) -> Option<TypeReference> {
        if let Some(element) = self.array_elements.get(type_name) {
            return Some(element.clone());
        }
        if type_name.is_array() == Some(true) {
            Some(type_name.element_type())
        } else {
            None
        }
    }
}

impl SequenceInfoRead for TestParseContext {
    fn get_sequence_impl(&self, key: &FullName) -> Option<&CreateSequence> {
        self.sequence_definitions.get(key)
    }
}

impl TestParseContext {
    /// setup [OperatorInfoRead::get_operator_op] result
    pub fn set_get_operator_op_result(
        &mut self,
        operator_name: &OperatorName,
        has_left: bool,
        has_right: bool,
        result: Option<&OperatorDefinition>,
    ) {
        self.operator_op_results.insert(
            (operator_name.clone(), has_left, has_right),
            result.cloned(),
        );
    }

    /// setup [OperatorInfoRead::get_operator_op] result
    pub fn setup_binary(
        &mut self,
        name: &str,
        left_type: &str,
        right_type: &str,
        ret_type: &str,
        strict: bool,
    ) {
        let name = OperatorName::Valid(ValidOperatorName::new(name));
        let binary = BinaryOperatorDefinition::new(
            &TypeReference::from_master_type_name(left_type),
            &TypeReference::from_master_type_name(right_type),
            &TypeReference::from_master_type_name(ret_type),
            strict,
        );
        let op_def = OperatorDefinition::Binary(vec![binary]);
        self.set_get_operator_op_result(&name, true, true, Some(&op_def));
    }
}

impl OperatorInfoRead for TestParseContext {
    fn get_operator_op(
        &self,
        name: &OperatorName,
        has_left: bool,
        has_right: bool,
    ) -> Option<&OperatorDefinition> {
        self.operator_op_results
            .get(&(name.clone(), has_left, has_right))?
            .as_ref()
    }
}

impl FunctionInfoRead for TestParseContext {
    fn is_aggregate_impl(&self, name: &FullName) -> bool {
        self.aggregate_functions.contains(name)
    }
    fn function_preserves_view_rows(
        &self,
        name: &FullName,
        overload: &crate::sem::OverloadVariant,
    ) -> bool {
        if let Some(preserves_rows) = self
            .function_row_properties
            .get(&(name.clone(), overload.clone()))
        {
            return *preserves_rows;
        }
        todo!("mock function-kind properties")
    }

    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        self.function_definitions.get(name).cloned()
    }
}

impl IndexInfoRead for TestParseContext {
    fn indexes_for_table(&self, table: &TableName) -> Vec<&CreateIndex> {
        self.create_indexes
            .iter()
            .filter(|index| index.belongs_to(table))
            .collect()
    }
    fn get_index_impl(&self, index_name: &FullName) -> Option<&CreateIndex> {
        self.create_indexes
            .iter()
            .rev()
            .find(|index| index.name() == index_name)
    }
}

impl DiagnosticContext for TestParseContext {
    #[track_caller]
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        if self.report_failure_after == Some(self.problems.len()) {
            return Err(std::io::Error::other("diagnostic sink rejected a problem").into());
        }
        self.problems.push(problem);
        Ok(())
    }
}

impl crate::sem::RuleInfoRead for TestParseContext {
    fn local_rule(
        &self,
        identity: &crate::sem::RuleIdentity,
    ) -> Option<Option<&crate::sem::CreateRule>> {
        self.rules.get(identity).map(Option::as_ref)
    }
    fn rule_parent(&self) -> Option<&dyn crate::sem::RuleInfoRead> {
        None
    }
}
impl crate::sem::RuleInfoWrite for TestParseContext {
    fn store_rule(
        &mut self,
        identity: &crate::sem::RuleIdentity,
        definition: Option<&crate::sem::CreateRule>,
    ) {
        self.rules.insert(identity.clone(), definition.cloned());
    }
    fn store_rule_view(&mut self, target: &FullName, view: &crate::sem::ViewLike) {
        self.view_definitions
            .insert(target.clone(), Some(view.clone()));
    }
}

impl crate::sem::TriggerInfoRead for TestParseContext {
    fn trigger_is_cancelled(&self, identity: &crate::sem::TriggerIdentity) -> bool {
        self.triggers.get(identity).is_some_and(Option::is_none)
    }
    fn trigger(
        &self,
        identity: &crate::sem::TriggerIdentity,
    ) -> Option<&crate::sem::CreateTrigger> {
        if let Some(trigger) = self.triggers.get(identity) {
            trigger.as_ref()
        } else {
            None
        }
    }
}

impl BaseParseContext for TestParseContext {
    fn sqlite_relation_identity(&self, name: &FullName) -> Option<FullName> {
        for (candidate, definition) in &self.table_definitions {
            if candidate.sqlite_eq(name) && definition.is_some() {
                return Some(candidate.clone());
            }
        }
        for (candidate, definition) in &self.view_definitions {
            if candidate.sqlite_eq(name) && definition.is_some() {
                return Some(candidate.clone());
            }
        }
        None
    }

    fn metadata(&self) -> &::sql_fun_core::SqlFunMetadata {
        static DEFAULT_METADATA: std::sync::LazyLock<::sql_fun_core::SqlFunMetadata> =
            std::sync::LazyLock::new(|| {
                ::sql_fun_core::SqlFunMetadataCollection::load_from(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/sql_fun.metadata.toml"
                ))
                .expect("default test metadata")
                .remove("default")
                .expect("default datasource")
            });
        self.metadata.as_ref().unwrap_or(&DEFAULT_METADATA)
    }

    fn get_args(&self) -> &sql_fun_core::SqlFunArgs {
        static ARGS: std::sync::LazyLock<::sql_fun_core::SqlFunArgs> =
            std::sync::LazyLock::new(|| {
                ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())
                    .unwrap()
                    .build()
                    .unwrap()
            });
        &ARGS
    }

    fn get_source_file_path(&self) -> Option<&std::path::Path> {
        todo!()
    }
}

impl TypeInfoWrite for TestParseContext {
    fn insert_table_ref_type(&mut self, name: &TableName) {
        self.table_ref_types.push(name.clone());
    }

    fn insert_create_composite_type(
        &mut self,
        _name: &FullName,
        _create_comp_type: &CreateCompositeType,
    ) {
        todo!()
    }

    fn insert_create_domain(&mut self, _name: &FullName, _create_domain: &CreateDomain) {
        todo!()
    }

    fn insert_create_enum(&mut self, _name: &FullName, _create_enum_type: &CreateEnum) {
        todo!()
    }
}

impl SequenceInfoWrite for TestParseContext {
    fn apply_alter_sequence(self, _alter_seq: &AlterSequence) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_create_sequence(
        mut self,
        create_sequence: &CreateSequence,
    ) -> Result<Self, AnalysisError> {
        let key = self.apply_default_schema(create_sequence.name())?;
        self.sequence_definitions
            .insert(key.clone(), create_sequence.with_catalog_name(&key));
        Ok(self)
    }
}

impl FunctionInfoWrite for TestParseContext {
    fn apply_function_view_rows(
        self,
        _function: &CreateFunction,
        _preserves_rows: bool,
    ) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_create_function_impl(
        self,
        _name: &FullName,
        _create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError> {
        todo!()
    }
}

impl crate::sem::TriggerInfoWrite for TestParseContext {
    fn register_trigger(
        mut self,
        trigger: &crate::sem::CreateTrigger,
    ) -> Result<Self, AnalysisError> {
        self.triggers
            .insert(trigger.identity().clone(), Some(trigger.clone()));
        Ok(self)
    }
    fn associate_trigger(
        mut self,
        target: &FullName,
        name: &FullName,
    ) -> Result<Self, AnalysisError> {
        if let Some(Some(table)) = self.table_definitions.get_mut(target) {
            table.associate_trigger(name);
        } else if let Some(Some(view)) = self.view_definitions.get_mut(target) {
            view.associate_trigger(name);
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
        self.triggers.insert(identity.clone(), None);
        Ok(self)
    }
    fn dissociate_trigger(
        mut self,
        target: &FullName,
        name: &FullName,
    ) -> Result<Self, AnalysisError> {
        if let Some(Some(table)) = self.table_definitions.get_mut(target) {
            table.dissociate_trigger(name);
        } else if let Some(Some(view)) = self.view_definitions.get_mut(target) {
            view.dissociate_trigger(name);
        } else {
            self.report_analysis_problem(crate::sem::AnalysisProblem::trigger(
                &crate::sem::problem::TriggerProblem::target(target),
            ))?;
        }
        Ok(self)
    }
}

impl crate::sem::ParameterTypeHintRead for TestParseContext {
    fn get_parameter_typehint(&self, index: usize) -> Option<TypeReference> {
        self.parameter_type_hints.lookup(index)
    }
}

impl crate::sem::ParameterTypeHintWrite for TestParseContext {
    fn setup_typehinting(&mut self, hints: &[Option<&str>]) -> Result<(), AnalysisError> {
        self.parameter_type_hints = crate::sem::ParameterTypeHints::resolve(hints, self)?;
        Ok(())
    }
}

impl ParseContext for TestParseContext {
    fn apply_creation_group<F>(
        self,
        operation: F,
    ) -> Result<crate::AstAndContextPair<Self>, AnalysisError>
    where
        F: FnOnce(Self) -> Result<crate::AstAndContextPair<Self>, AnalysisError>,
    {
        // Schema publication tests use SchemaFileContext. This fixture records
        // direct calls rather than implementing the production catalog layers.
        operation(self)
    }
    fn insert_role(&mut self, role: &crate::sem::Role) {
        self.ownership.insert_role(role);
    }
    fn insert_owned_object(&mut self, definition: &crate::sem::OwnedObjectDefinition) {
        self.ownership.insert(definition);
    }
    fn apply_alter_owner(
        mut self,
        alteration: &crate::sem::AlterOwner,
    ) -> Result<Self, AnalysisError> {
        if let Some(identity) = alteration.applied_identity() {
            let definition =
                crate::sem::OwnedObjectDefinition::new(identity, Some(alteration.owner()));
            self.ownership.insert(&definition);
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

    fn freeze(mut self) -> crate::sem::FrozenBaseContext {
        self.problems.clear();
        self.parameter_constraints = crate::sem::ParameterConstraints::default();
        self.parameter_type_hints = Default::default();
        self.report_failure_after = None;
        crate::sem::FrozenBaseContext::new(self)
    }

    fn set_search_path(&mut self, search_path: &[SchemaName]) {
        self.search_path = search_path.to_vec();
    }

    fn set_statement_span(&mut self, _statement_span: StringSpan) {
        todo!()
    }

    fn set_source_file(
        &mut self,
        _source_access_service: Box<dyn crate::sem::SourceAccessService>,
    ) {
        todo!()
    }

    fn apply_comment(self, _comment: &Comment) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_create_extension(
        self,
        _create_extension: &CreateExtension,
    ) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_variable_set(self, _variable_set: &VariableSet) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_create_schema(self, _create_schema: &CreateSchema) -> Result<Self, AnalysisError> {
        todo!()
    }

    fn apply_alter_index(mut self, index: &crate::sem::AlterIndex) -> Result<Self, AnalysisError> {
        if let Some(replacement) = index.replacement() {
            self.create_indexes
                .retain(|existing| existing.name() != replacement.name());
            self.create_indexes.push(replacement.clone());
        }
        Ok(self)
    }
    fn apply_create_index(self, _create_index: &CreateIndex) -> Result<Self, AnalysisError> {
        let mut context = self;
        context.create_indexes.push(_create_index.clone());
        Ok(context)
    }
}

#[cfg(test)]
mod test_search_path {
    use super::TestParseContext;
    use crate::{
        context::ParseContext,
        sem::{SchemaName, SearchPathRead},
    };

    #[test]
    fn replaces_search_path_and_updates_default_schema() {
        let mut context = TestParseContext::default();
        context.set_search_path(&[SchemaName::from("public")]);
        let replacement = vec![SchemaName::from("app"), SchemaName::from("pg_catalog")];
        context.set_search_path(&replacement);
        assert_eq!(context.get_search_path(), replacement);
        assert_eq!(context.get_default_schema(), Some(SchemaName::from("app")));

        context.set_search_path(&[]);
        assert!(context.get_search_path().is_empty());
        assert_eq!(context.get_default_schema(), None);
    }
}

impl TestParseContext {
    /// setup [FunctionInfoRead::get_function_impl] result
    pub fn set_get_function_impl_result(
        &mut self,
        name: &FullName,
        overloads: &FunctionOverloadCollection,
    ) {
        self.function_definitions
            .insert(name.clone(), overloads.clone());
    }

    pub(crate) fn setup_aggregate(&mut self, name: &str, overloads: &[OverloadVariant]) {
        let name = self.setup_function(name, overloads);
        self.aggregate_functions.insert(name);
    }
    pub(crate) fn set_function_row_property(
        &mut self,
        name: &FullName,
        overload: &OverloadVariant,
        preserves_rows: bool,
    ) {
        self.function_row_properties
            .insert((name.clone(), overload.clone()), preserves_rows);
    }

    /// setup function overload collection with variants
    pub fn setup_function(&mut self, name: &str, overloads: &[OverloadVariant]) -> FullName {
        let name = FullName::with_schema("public", name);
        let overloads = FunctionOverloadCollection::new(overloads);
        self.set_get_function_impl_result(&name, &overloads);
        name
    }

    /// number of reported problems
    #[must_use]
    pub fn reported_problem_count(&self) -> usize {
        self.problems.len()
    }
}

impl TestParseContext {
    /// Build a scalar expression's data-source scope from a SQL FROM body.
    pub fn scalar_scope_from_sql(from_sql: &str) -> Result<(FromClause, Self), AnalysisError> {
        let mut context = Self::default();
        context.set_source_path_with_span(&source_span());
        let syn = SynBuilder::new().parse(&format!("select 1 from {from_sql}"));
        FromClause::analyze(
            context,
            &WithClause::default(),
            syn.as_select_stmt().get_from_clause(),
            &sql_fun_core::IVec::default(),
        )
    }
}

impl TestParseContext {
    pub(crate) fn set_view_insert_support(
        &mut self,
        name: &FullName,
        support: &crate::sem::ViewInsertSupport,
    ) {
        self.view_insert_support
            .insert(name.clone(), support.clone());
    }
}
impl crate::sem::ViewInsertInfoRead for TestParseContext {
    fn view_insert_support(&self, view: &FullName) -> Option<&crate::sem::ViewInsertSupport> {
        self.view_insert_support.get(view)
    }
}

impl crate::sem::ViewUpdateInfoRead for TestParseContext {
    fn view_update_support(&self, view: &FullName) -> Option<&crate::sem::ViewUpdateSupport> {
        self.view_update_support.get(view)
    }
}

impl TestParseContext {
    pub(crate) fn set_view_update_support(
        &mut self,
        name: &FullName,
        support: &crate::sem::ViewUpdateSupport,
    ) {
        self.view_update_support
            .insert(name.clone(), support.clone());
    }
}

impl crate::sem::ViewDeleteInfoRead for TestParseContext {
    fn view_delete_support(&self, view: &FullName) -> crate::sem::ViewDeleteSupport {
        self.view_delete_support
            .get(view)
            .expect("explicit view DELETE capability fixture")
            .clone()
    }
}

impl crate::sem::ViewMergeInfoRead for TestParseContext {
    fn view_merge_support(&self, view: &FullName) -> crate::sem::ViewMergeSupport {
        self.view_merge_support
            .get(view)
            .expect("explicit view MERGE capability fixture")
            .clone()
    }
}

impl TestParseContext {
    pub(crate) fn set_view_delete_support(
        &mut self,
        name: &FullName,
        support: &crate::sem::ViewDeleteSupport,
    ) {
        self.view_delete_support
            .insert(name.clone(), support.clone());
    }

    pub(crate) fn set_view_merge_support(
        &mut self,
        name: &FullName,
        support: &crate::sem::ViewMergeSupport,
    ) {
        self.view_merge_support
            .insert(name.clone(), support.clone());
    }
}

impl crate::sem::RoleInfoRead for TestParseContext {
    fn role_by_name(&self, name: &str) -> Option<&crate::sem::Role> {
        self.ownership.role(name)
    }
}
impl crate::sem::OwnershipInfoRead for TestParseContext {
    fn ownership_definitions(
        &self,
        kind: crate::sem::ObjectKind,
        name: &FullName,
    ) -> Vec<crate::sem::OwnedObjectDefinition> {
        let supplied = self.ownership.definitions(kind, name);
        if supplied.is_empty() {
            crate::sem::ownership::model_definitions(self, kind, name)
        } else {
            supplied
        }
    }
    fn routine_definitions(&self, name: &FullName) -> Vec<CreateFunction> {
        self.get_function_impl(name)
            .map_or_else(Vec::new, |overloads| {
                CreateFunction::new(name, &overloads, &None).split_definitions()
            })
    }
}

impl crate::sem::ObjectInfoRead for TestParseContext {
    fn object_kind(&self, name: &crate::sem::FullName) -> Option<crate::sem::Kind> {
        if self.get_table_impl(name).is_some() {
            Some(crate::sem::Kind::table())
        } else if self.get_view_impl(name).is_some() {
            Some(crate::sem::Kind::view())
        } else if self.get_sequence_impl(name).is_some() {
            Some(crate::sem::Kind::sequence())
        } else if self.get_index_impl(name).is_some() {
            Some(crate::sem::Kind::index())
        } else {
            None
        }
    }
}
impl crate::sem::ExtensionInfoRead for TestParseContext {
    fn extension_exists(&self, _: &str) -> bool {
        false
    }
}
