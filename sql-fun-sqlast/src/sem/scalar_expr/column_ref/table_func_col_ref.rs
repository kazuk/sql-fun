use crate::{
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, AnalysisProblem, ColumnDefinition, ColumnName, Nullable,
        OverloadVariant, SemScalarExpr, TypeReference, WithTypmod,
        data_source::AliasName,
        scalar_expr::{ColumnReferenceExpr, column_ref::ColumnReference},
    },
};

use ::sql_fun_core::IVec;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableFunctionColumnRef {
    alias: AliasName,
    column: ColumnName,
    column_def: Option<ColumnDefinition>,
    type_declaration: Option<WithTypmod<TypeReference>>,
    table_function: Option<OverloadVariant>,
    args: IVec<SemScalarExpr>,
    datasource_nullable: Nullable,
}

impl ColumnReference for TableFunctionColumnRef {
    fn get_type(&self) -> Option<TypeReference> {
        if let Some(col_def) = &self.column_def {
            col_def.get_type_cloned()
        } else {
            None
        }
    }

    fn is_not_null(&self) -> Option<bool> {
        if self.datasource_nullable {
            return Some(false);
        }
        let Some(column_def) = &self.column_def else {
            return None;
        };
        column_def.is_not_null()
    }

    fn get_column_def(&self) -> &Option<ColumnDefinition> {
        &self.column_def
    }

    fn get_column_name(&self) -> &ColumnName {
        &self.column
    }
}

impl TableFunctionColumnRef {
    /// Declaration-local modifiers retained from the table invocation.
    pub const fn type_declaration(&self) -> Option<&WithTypmod<TypeReference>> {
        self.type_declaration.as_ref()
    }
    pub(super) fn with_reference_name(mut self, alias: &AliasName, column: &ColumnName) -> Self {
        self.alias = alias.clone();
        self.column = column.clone();
        self
    }

    pub(super) const fn with_nullability(mut self, nullable: bool) -> Self {
        self.datasource_nullable |= nullable;
        self
    }

    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.column_def = None;
    }
    fn new(
        alias: &AliasName,
        column: &ColumnName,
        column_def: Option<&ColumnDefinition>,
        table_function: Option<&OverloadVariant>,
        args: &IVec<SemScalarExpr>,
        datasource_nullable: Nullable,
    ) -> Self {
        Self {
            alias: alias.clone(),
            column: column.clone(),
            column_def: column_def.cloned(),
            type_declaration: None,
            table_function: table_function.cloned(),
            args: args.clone(),
            datasource_nullable,
        }
    }

    pub(crate) fn function_source_ref<'context, TParseContext>(
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        calling_func: Option<&OverloadVariant>,
        column_def: Option<&ColumnDefinition>,
        args: &IVec<SemScalarExpr>,
        nullability: Nullable,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        if column_def.is_none() {
            context.report_analysis_problem(
                AnalysisProblem::column_not_found_in_table_function(alias, column),
            )?;
        }
        let col_ref = Self::new(alias, column, column_def, calling_func, args, nullability);
        Ok((
            SemScalarExpr::column_reference(ColumnReferenceExpr::table_function(col_ref)),
            context,
        ))
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn function_source_ref<'context, TParseContext>(
        context: &'context mut TParseContext,
        alias: &AliasName,
        column: &ColumnName,
        calling_func: Option<&OverloadVariant>,
        column_def: Option<&ColumnDefinition>,
        args: &IVec<SemScalarExpr>,
        nullability: Nullable,
    ) -> Result<(SemScalarExpr, &'context mut TParseContext), AnalysisError>
    where
        TParseContext: ParseContext,
    {
        TableFunctionColumnRef::function_source_ref(
            context,
            alias,
            column,
            calling_func,
            column_def,
            args,
            nullability,
        )
    }
}

impl ColumnReferenceExpr {
    pub(crate) fn function_output(
        alias: &AliasSpec,
        definition: &ColumnDefinition,
    ) -> SemScalarExpr {
        let name = definition
            .name()
            .as_ref()
            .expect("function output has a name");
        let reference = TableFunctionColumnRef::new(
            alias.name(),
            name,
            Some(definition),
            None,
            &IVec::default(),
            false,
        );
        SemScalarExpr::column_reference(Self::TableFuncColumn(reference))
    }
}

impl ColumnReferenceExpr {
    fn with_function_source_nullability(self, nullable: bool) -> Self {
        match self {
            Self::TableFuncColumn(reference) => {
                Self::TableFuncColumn(reference.with_nullability(nullable))
            }
            _ => self,
        }
    }
    const fn table_function(reference: TableFunctionColumnRef) -> Self {
        Self::TableFuncColumn(reference)
    }
}
impl SemScalarExpr {
    pub(crate) fn with_function_source_nullability(self, nullable: bool) -> Self {
        match self {
            Self::ColumnRef(reference) => {
                Self::column_reference(reference.with_function_source_nullability(nullable))
            }
            _ => self,
        }
    }
}

impl SemScalarExpr {
    pub(crate) fn with_function_type_declaration(
        mut self,
        declaration: Option<&WithTypmod<TypeReference>>,
    ) -> Self {
        if let Self::ColumnRef(ColumnReferenceExpr::TableFuncColumn(reference)) = &mut self {
            reference.type_declaration = declaration.cloned();
        }
        self
    }
}

impl TableFunctionColumnRef {
    pub(super) const fn projection_reference(&self) -> Option<(&str, &ColumnName)> {
        Some((self.alias.as_str(), &self.column))
    }
}
