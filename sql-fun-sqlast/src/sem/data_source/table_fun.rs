use crate::{
    context::ParseContext,
    sem::{
        AliasSpec, AnalysisError, ColumnCollection, ColumnName, ColumnReferenceExpr, DataSource,
        FullName, Nullable, OverloadVariant, QueryName, QueryNameResolution, SemScalarExpr,
        TableFunctionProblem, TypeReference, WildcardTypedRecord, WithClause, WithTypmod,
        data_source::AliasName, traversal::SemanticVisitorMut,
    },
};

use ::sql_fun_core::IVec;

use super::TableFunctionOutput;

#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TableFunctionDataSource {
    function: OverloadVariant,
    function_name: Option<QueryName>,
    args: IVec<SemScalarExpr>,
    alias: AliasSpec,
    output: TableFunctionOutput,
}

impl super::DataSource {
    /// create datasource for table function
    pub(crate) fn table_function(
        overload: &OverloadVariant,
        arg_exprs: IVec<SemScalarExpr>,
        alias_name: &AliasSpec,
    ) -> Self {
        TableFunctionDataSource::new_data_source(overload.clone(), arg_exprs, alias_name)
    }
}

impl TableFunctionDataSource {
    /// Requested function name and its recorded search-path resolution.
    pub const fn function_name(&self) -> Option<&QueryName> {
        self.function_name.as_ref()
    }
    pub const fn function(&self) -> &OverloadVariant {
        &self.function
    }

    pub const fn arguments(&self) -> &IVec<SemScalarExpr> {
        &self.args
    }

    pub(super) fn new_data_source(
        function: OverloadVariant,
        args: IVec<SemScalarExpr>,
        alias: &AliasSpec,
    ) -> DataSource {
        DataSource::CallTableFunc(Self {
            function_name: None,
            output: TableFunctionOutput::declared(&function),
            function,
            args,
            alias: alias.clone(),
        })
    }

    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }

    pub(super) fn resolve_alias<'a>(
        &self,
        data_source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((data_source, false))
        } else {
            None
        }
    }

    pub(super) fn get_aliases_for_column<TParseContext>(
        &self,
        column: &ColumnName,
        _context: &TParseContext,
        _with_clause: &WithClause,
        names: &mut Vec<AliasSpec>,
    ) where
        TParseContext: ParseContext,
    {
        names.extend(self.output.candidates(column, &self.alias));
    }
}

impl TableFunctionDataSource {
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        let mut args = self.args.to_vec();
        for argument in &mut args {
            argument.walk(visitor)?;
        }
        self.args = args.into();
        Ok(())
    }
    pub(crate) fn visit<V: crate::sem::traversal::SemanticVisitor>(
        &self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        if let Some(name) = &self.function_name
            && let Some(name) = name.resolution().function_identity()
        {
            visitor.function_binding(name, &self.alias, &self.output.names());
        }
        for argument in self.args.iter() {
            argument.visit(visitor)?;
        }
        Ok(())
    }
}

impl TableFunctionDataSource {
    pub(super) fn apply_output(
        &mut self,
        context: &mut impl ParseContext,
        output: &TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        self.output.replace_definition(output);
        self.output.apply(context, &self.function, &self.alias)
    }
    pub(super) fn column_names(&self) -> Vec<ColumnName> {
        self.output.names()
    }
    pub(super) fn bind_column<'a, C: ParseContext>(
        &self,
        context: &'a mut C,
        alias: &AliasName,
        column: &ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'a mut C), AnalysisError> {
        if self.output.candidates(column, &self.alias).len() > 1 {
            return Ok((
                ColumnReferenceExpr::ambiguous_column_resolved(context, column)?,
                context,
            ));
        }
        let (expression, context) = ColumnReferenceExpr::function_source_ref(
            context,
            alias,
            column,
            Some(&self.function),
            self.output.column(column, &self.alias),
            &self.args,
            nullable,
        )?;
        Ok((
            expression.with_function_type_declaration(self.output.declaration(column, &self.alias)),
            context,
        ))
    }
}

impl TableFunctionDataSource {
    pub(super) fn append_ordinality(&mut self) {
        self.output.append_ordinality();
    }
    pub(super) fn validate_aliases(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        self.output.validate_aliases(context, &self.alias)
    }
}

impl TableFunctionDataSource {
    /// Caller-supplied type and modifiers for an effective output column.
    pub fn output_type_declaration(
        &self,
        column: &ColumnName,
    ) -> Option<&WithTypmod<TypeReference>> {
        self.output.declaration(column, &self.alias)
    }
    pub(super) fn output_type_declaration_at(
        &self,
        index: usize,
    ) -> Option<&WithTypmod<TypeReference>> {
        self.output.declaration_at(index)
    }

    /// Effective invocation columns, independently of the catalog return declaration.
    pub const fn output_columns(&self) -> &ColumnCollection {
        self.output.columns()
    }
    /// Retained invalid-definition outcomes, independently of diagnostic delivery.
    pub fn output_problems(&self) -> &[TableFunctionProblem] {
        self.output.problems()
    }
}

impl DataSource {
    pub(super) const fn resolved_function_name(&self) -> Option<&QueryName> {
        if let Self::CallTableFunc(source) = self {
            source.function_name()
        } else {
            None
        }
    }
    pub(crate) fn record_function_name(&mut self, requested: &FullName, resolved: &FullName) {
        if let Self::CallTableFunc(source) = self {
            source.function_name = Some(QueryName::new(
                &requested.to_string(),
                QueryNameResolution::function(resolved),
            ));
        }
    }
}

impl TableFunctionDataSource {
    pub(super) fn typed_projection_record(
        &self,
        alias: &AliasName,
        context: &impl ParseContext,
        index: usize,
    ) -> Option<WildcardTypedRecord> {
        WildcardTypedRecord::from_function(alias, &self.function, context, index)
    }

    pub(super) fn bind_projection_ordinal(
        &self,
        context: &mut impl ParseContext,
        alias: &AliasName,
        column: &ColumnName,
        index: usize,
        nullable: bool,
    ) -> Result<SemScalarExpr, AnalysisError> {
        let (value, _) = ColumnReferenceExpr::function_source_ref(
            context,
            alias,
            column,
            Some(&self.function),
            self.output_columns().column_at(index),
            &self.args,
            nullable,
        )?;
        Ok(value.with_function_type_declaration(self.output_type_declaration_at(index)))
    }
}

impl TableFunctionDataSource {
    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        self.function.table_output_depends_on_table(context)
    }
}

impl DataSource {
    pub(crate) fn validate_table_function(
        &mut self,
        context: &mut impl ParseContext,
        invalid_arguments: bool,
    ) -> Result<(), AnalysisError> {
        let invalid = match self {
            Self::CallTableFunc(source) => source.invalid_from(context),
            Self::DeferredTableFunc(source) => source.call().invalid_from(context, true),
            _ => false,
        };
        if invalid {
            context.report_analysis_problem(crate::sem::AnalysisProblem::table_function(
                TableFunctionProblem::invalid_expression(),
            ))?;
        }
        if invalid || invalid_arguments {
            match self {
                Self::CallTableFunc(source) => source.output.retain_invalid_expression(),
                Self::DeferredTableFunc(source) => source.retain_invalid_expression(),
                _ => {}
            }
        }
        Ok(())
    }
}

impl TableFunctionDataSource {
    fn invalid_from(&self, context: &impl ParseContext) -> bool {
        if let Some(requested) = self.function_name()
            && let Some(name) = requested.resolution().function_identity()
        {
            !context.function_returns_set(name, self.function())
                && !context.function_preserves_view_rows(name, self.function())
        } else {
            false
        }
    }
}
