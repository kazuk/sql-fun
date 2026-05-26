mod cast_info;

use std::collections::{HashMap, HashSet};

use crate::{
    StringSpan,
    sem::{
        AlterSequence, AlterTable, AlterView, AnalysisError, AnalysisProblem, BaseParseContext,
        BinaryOperatorDefinition, CastContext, CastDefinition, ColumnDefinition, Comment,
        CreateCompositType, CreateDomain, CreateEnum, CreateExtension, CreateFunction, CreateIndex,
        CreateSchema, CreateSequence, CreateTable, CreateTableAs, CreateTableMode, CreateView,
        DropTable, FullName, FunctionInfoRead, FunctionInfoWrite, FunctionOverloadCollection,
        GetSourcePathWithSpanOpt, IndexInfoRead, MutSourcePathWithSpan, OperatorDefinition,
        OperatorInfoRead, OperatorName, OverloadVariant, ParseContext, SchemaName, SearchPathRead,
        SequenceInfoRead, SequenceInfoWrite, SourcePathWithSpan, TableInfoRead, TableInfoWrite,
        TableName, TypeDefinition, TypeInfoRead, TypeInfoWrite, TypeReference, ValidOperatorName,
        VariableSet, ViewInfoRead, ViewInfoWrite, ViewLike,
    },
};

/// ParseContext implementation for tests
#[derive(Debug, Default)]
pub struct TestParseContext {
    implicit_cast_results: HashMap<TypeReference, HashMap<TypeReference, Option<CastDefinition>>>,
    implicit_castable_results: HashMap<TypeReference, HashSet<TypeReference>>,
    operator_op_results: HashMap<(OperatorName, bool, bool), Option<OperatorDefinition>>,
    search_path: Vec<SchemaName>,
    type_definitions: HashMap<FullName, TypeDefinition>,
    table_definitions: HashMap<FullName, Option<CreateTable>>,
    view_definitions: HashMap<FullName, Option<ViewLike>>,
    function_definitions: HashMap<FullName, FunctionOverloadCollection>,
    alter_tables: Vec<AlterTable>,
    drop_tables: Vec<DropTable>,
    create_indexes: Vec<CreateIndex>,
    problems: Vec<AnalysisProblem>,
    source: Option<SourcePathWithSpan>,
}

/// get [TestParseContext]
#[rstest::fixture]
pub fn test_context() -> TestParseContext {
    TestParseContext::default()
}

impl TestParseContext {
    /// setup [SearchPathRead::get_search_path] result
    pub fn set_get_search_path_result(&mut self, search_path: &Vec<SchemaName>) {
        self.search_path = search_path.clone()
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
    fn set_search_path(&mut self, _search_path: Vec<SchemaName>) {
        todo!()
    }

    fn get_search_path(&self) -> &[SchemaName] {
        self.search_path.as_slice()
    }

    fn get_default_schema(&self) -> Option<SchemaName> {
        todo!()
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

    /// setup [TableInfoRead::get_table_impl] resut
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

    fn insert_create_table(&mut self, _name: &FullName, _create_table: &CreateTable) {
        todo!()
    }
}

impl TypeInfoRead for TestParseContext {
    fn get_type_impl(&self, key: &FullName) -> Option<&TypeDefinition> {
        self.type_definitions.get(key)
    }

    fn get_array_item_type(&self, _type_name: &TypeReference) -> Option<TypeReference> {
        todo!()
    }
}

impl SequenceInfoRead for TestParseContext {
    fn get_sequence_impl(&self, _key: &FullName) -> Option<&CreateSequence> {
        todo!()
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
    fn get_function_impl(&self, name: &FullName) -> Option<FunctionOverloadCollection> {
        self.function_definitions.get(name).cloned()
    }
}

impl IndexInfoRead for TestParseContext {
    fn get_index(&self, _index_name: &str) -> Option<&CreateIndex> {
        todo!()
    }
}

impl BaseParseContext for TestParseContext {
    #[track_caller]
    fn report_problem(&mut self, problem: AnalysisProblem) -> Result<(), AnalysisError> {
        self.problems.push(problem);
        Ok(())
    }

    fn extend(&mut self, _extension: Box<dyn BaseParseContext>) {
        todo!()
    }

    fn set_statement_span(&mut self, _statement_span: StringSpan) {
        todo!()
    }

    fn get_args(&self) -> &sql_fun_core::SqlFunArgs {
        todo!()
    }

    fn get_source_file_path(&self) -> Option<&std::path::Path> {
        todo!()
    }

    fn set_source_file(&mut self, _source_acess_service: Box<dyn crate::sem::SourceAccessService>) {
        todo!()
    }
}

impl TypeInfoWrite for TestParseContext {
    fn insert_table_ref_type(&mut self, _name: &TableName) {
        todo!()
    }

    fn insert_create_composite_type(
        &mut self,
        _name: &FullName,
        _create_comp_type: &CreateCompositType,
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
        self,
        _create_sequence: &CreateSequence,
    ) -> Result<Self, AnalysisError> {
        todo!()
    }
}

impl FunctionInfoWrite for TestParseContext {
    fn apply_create_function_impl(
        self,
        _name: &FullName,
        _create_function: &CreateFunction,
    ) -> Result<Self, AnalysisError> {
        todo!()
    }
}

impl ParseContext for TestParseContext {
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

    fn apply_create_index(self, _create_index: &CreateIndex) -> Result<Self, AnalysisError> {
        let mut context = self;
        context.create_indexes.push(_create_index.clone());
        Ok(context)
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
