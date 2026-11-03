use crate::{
    context::ParseContext,
    names::{AliasName, ColumnName},
    sem::{
        AliasSpec, AnalysisError, ColumnCollection, ColumnReferenceExpr, DataSource, FuncCallExpr,
        Nullable, SemScalarExpr, TableFunctionProblem, TypeReference, WithTypmod,
        data_source::{ColumnCandidates, TableFunctionOutput},
        traversal::{SemanticVisitor, SemanticVisitorMut},
    },
};

/// A table invocation with completed name lookup and deferred signature/columns.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeferredTableFunction {
    call: FuncCallExpr,
    alias: AliasSpec,
    output: TableFunctionOutput,
}
impl DeferredTableFunction {
    pub(super) fn retain_invalid_expression(&mut self) {
        self.output.retain_invalid_expression();
    }
    pub(super) fn new(call: &FuncCallExpr, alias: &AliasSpec) -> Self {
        Self {
            call: call.clone(),
            alias: alias.clone(),
            output: TableFunctionOutput::default(),
        }
    }
    /// Preserved invocation, including name lookup, supplied arguments and modifiers.
    pub const fn call(&self) -> &FuncCallExpr {
        &self.call
    }
    /// Explicit or SQL-derived table alias.
    pub const fn alias(&self) -> &AliasSpec {
        &self.alias
    }
    pub(super) fn push_alias_names(&self, names: &mut Vec<AliasSpec>) {
        names.push(self.alias.clone());
    }
    pub(super) fn resolve_alias<'a>(
        &self,
        source: &'a DataSource,
        name: &AliasName,
    ) -> Option<(&'a DataSource, Nullable)> {
        if self.alias.name() == name {
            Some((source, false))
        } else {
            None
        }
    }
    pub(super) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.call.walk(visitor)
    }
    pub(super) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        if let Some(name) = self.call.name_resolution().function_identity() {
            visitor.function_binding(name, &self.alias, &self.output.names());
        }
        self.call.visit(visitor)
    }
}
impl DataSource {
    pub(crate) fn deferred_table_function(call: &FuncCallExpr, alias: &AliasSpec) -> Self {
        Self::DeferredTableFunc(DeferredTableFunction::new(call, alias))
    }
    /// Table invocation whose signature and output columns remain deferred.
    pub const fn deferred_function(&self) -> Option<&DeferredTableFunction> {
        match self {
            Self::DeferredTableFunc(value) => Some(value),
            _ => None,
        }
    }
}

impl DeferredTableFunction {
    pub(super) fn apply_output(
        &mut self,
        context: &mut impl ParseContext,
        output: &TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        self.output.replace_definition(output);
        if let Some(overload) = self.call.overload() {
            self.output.apply(context, overload, &self.alias)?;
        } else if let Some(candidates) = self.call.candidates()
            && let Some(required) = candidates.record_definition_requirement()
        {
            self.output.validate_requirement(context, required)?;
        }
        Ok(())
    }
    pub(super) fn column_names(&self) -> Vec<ColumnName> {
        self.output.names()
    }
    pub(super) fn candidates(&self, column: &ColumnName) -> ColumnCandidates {
        if self.output.has_declarations() {
            ColumnCandidates::from(self.output.candidates(column, &self.alias))
        } else {
            ColumnCandidates::from(vec![self.alias.clone()])
        }
    }
    pub(super) fn bind_column<'a, C: ParseContext>(
        &self,
        context: &'a mut C,
        alias: &AliasName,
        column: &ColumnName,
        nullable: bool,
    ) -> Result<(SemScalarExpr, &'a mut C), AnalysisError> {
        if !self.output.has_declarations() {
            return ColumnReferenceExpr::unknown_function_source_ref(context, alias, column);
        }
        let (expression, context) = ColumnReferenceExpr::function_source_ref(
            context,
            alias,
            column,
            self.call.overload().as_ref(),
            self.output.column(column, &self.alias),
            &self.call.argument_values(),
            nullable,
        )?;
        Ok((
            expression.with_function_type_declaration(self.output.declaration(column, &self.alias)),
            context,
        ))
    }
}

impl DeferredTableFunction {
    pub(super) fn append_ordinality(&mut self) {
        self.output.append_ordinality();
    }
    pub(super) fn validate_aliases(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if self.output.has_declarations() {
            self.output.validate_aliases(context, &self.alias)
        } else {
            Ok(())
        }
    }
}

impl DeferredTableFunction {
    /// Caller-supplied type and modifiers for an effective output column.
    pub fn output_type_declaration(
        &self,
        column: &ColumnName,
    ) -> Option<&WithTypmod<TypeReference>> {
        self.output.declaration(column, &self.alias)
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

impl DeferredTableFunction {
    pub(super) fn known_column_names(&self) -> Option<Vec<ColumnName>> {
        if self.output.has_declarations() {
            Some(self.output.names())
        } else {
            None
        }
    }
}

impl DeferredTableFunction {
    pub(super) fn projection_shape_is_resolved(&self) -> bool {
        self.known_column_names().is_some()
    }

    pub(super) fn projection_is_table_dependent(&self, context: &impl ParseContext) -> bool {
        if let Some(overload) = self.call().overload().as_ref() {
            overload.table_output_depends_on_table(context)
        } else {
            false
        }
    }
}
