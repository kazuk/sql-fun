mod cast_info;

use std::collections::HashMap;

use crate::{
    StringSpan,
    sem::{
        AlterSequence, AlterTable, AlterView, AnalysisError, AnalysisProblem, BaseParseContext,
        BinaryOperatorDefinition, CastContext, CastDefinition, ColumnDefinition, Comment,
        CreateCompositType, CreateDomain, CreateEnum, CreateExtension, CreateFunction, CreateIndex,
        CreateSchema, CreateSequence, CreateTable, CreateTableAs, CreateView, FullName,
        FunctionInfoRead, FunctionInfoWrite, FunctionOverloadCollection, IndexInfoRead,
        OperatorDefinition, OperatorInfoRead, OperatorName, ParseContext, SchemaName,
        SearchPathRead, SequenceInfoRead, SequenceInfoWrite, TableInfoRead, TableInfoWrite,
        TableName, TypeDefinition, TypeInfoRead, TypeInfoWrite, TypeReference, ValidOperatorName,
        VariableSet, ViewInfoRead, ViewInfoWrite, ViewLike,
    },
};

/// ParseContext implementation for tests
#[derive(Debug, Default)]
pub struct TestParseContext {
    implicit_cast_results: HashMap<TypeReference, HashMap<TypeReference, Option<CastDefinition>>>,
    operator_op_results: HashMap<(OperatorName, bool, bool), Option<OperatorDefinition>>,
    search_path: Vec<SchemaName>,
    type_definitions: HashMap<FullName, TypeDefinition>,
    table_definitions: HashMap<FullName, Option<CreateTable>>,
    view_definitions: HashMap<FullName, Option<ViewLike>>,
    alter_tables: Vec<AlterTable>
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
    pub fn setup_table(&mut self, table_name: &str, columns: &[ColumnDefinition]) {
        let table_full_name = FullName::with_schema("public", table_name);
        let table_definition =
            CreateTable::new(TableName::from(table_full_name.clone()), columns.to_vec());
        self.set_get_table_impl_result(&table_full_name, &Some(table_definition));
    }

    /// get alter table command length
    pub fn alter_tables_len(&self) -> usize {
        self.alter_tables.len()
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
    fn get_function_impl(&self, _name: &FullName) -> Option<FunctionOverloadCollection> {
        todo!()
    }
}

impl IndexInfoRead for TestParseContext {
    fn get_index(&self, _index_name: &str) -> Option<&CreateIndex> {
        todo!()
    }
}

impl BaseParseContext for TestParseContext {
    #[track_caller]
    fn report_problem(&mut self, problem: AnalysisProblem) {
        todo!("{problem:#?}")
    }

    fn extend(&mut self, _extension: Box<dyn BaseParseContext>) {
        todo!()
    }

    fn set_statement_span(&mut self, _statement_span: StringSpan) {
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
        todo!()
    }
}
