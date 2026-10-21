use crate::sem::{AliasName, AliasSpec, AnalysisError, DataSource, FuncCallExpr, Nullable};

/// A table invocation with completed name lookup and deferred signature/columns.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeferredTableFunction {
    call: FuncCallExpr,
    alias: AliasSpec,
    output: super::TableFunctionOutput,
}
impl DeferredTableFunction {
    pub(super) fn new(call: &FuncCallExpr, alias: &AliasSpec) -> Self {
        Self {
            call: call.clone(),
            alias: alias.clone(),
            output: super::TableFunctionOutput::default(),
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
    pub(super) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
        self.call.walk(visitor)
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
        context: &mut impl crate::sem::ParseContext,
        output: &super::TableFunctionOutput,
    ) -> Result<(), AnalysisError> {
        self.output = output.clone();
        if let Some(overload) = self.call.overload() {
            self.output.apply(context, overload, &self.alias)?;
        } else if let Some(candidates) = self.call.candidates()
            && let Some(required) = candidates.record_definition_requirement()
        {
            self.output.validate_requirement(context, required)?;
        }
        Ok(())
    }
    pub(super) fn column_names(&self) -> Vec<crate::sem::ColumnName> {
        self.output.names()
    }
    pub(super) fn candidates(&self, column: &crate::sem::ColumnName) -> super::ColumnCandidates {
        if self.output.has_declarations() {
            super::ColumnCandidates::from(self.output.candidates(column, &self.alias))
        } else {
            super::ColumnCandidates::from(vec![self.alias.clone()])
        }
    }
    pub(super) fn bind_column<'a, C: crate::sem::ParseContext>(
        &self,
        context: &'a mut C,
        alias: &AliasName,
        column: &crate::sem::ColumnName,
        nullable: bool,
    ) -> Result<(crate::sem::SemScalarExpr, &'a mut C), AnalysisError> {
        if !self.output.has_declarations() {
            return crate::sem::ColumnReferenceExpr::unknown_function_source_ref(
                context, alias, column,
            );
        }
        let mut arguments = Vec::new();
        for argument in self.call.args() {
            arguments.push(argument.value().clone());
        }
        crate::sem::ColumnReferenceExpr::function_source_ref(
            context,
            alias,
            column,
            self.call.overload().as_ref(),
            self.output.column(column, &self.alias),
            &arguments.into(),
            nullable,
        )
    }
}

impl DeferredTableFunction {
    pub(super) fn append_ordinality(&mut self) {
        self.output.append_ordinality();
    }
    pub(super) fn validate_aliases(
        &mut self,
        context: &mut impl crate::sem::ParseContext,
    ) -> Result<(), crate::sem::AnalysisError> {
        if self.output.has_declarations() {
            self.output.validate_aliases(context, &self.alias)
        } else {
            Ok(())
        }
    }
}

impl DeferredTableFunction {
    /// Effective invocation columns, independently of the catalog return declaration.
    pub const fn output_columns(&self) -> &crate::sem::ColumnCollection {
        self.output.columns()
    }
    /// Retained invalid-definition outcomes, independently of diagnostic delivery.
    pub fn output_problems(&self) -> &[crate::sem::TableFunctionProblem] {
        self.output.problems()
    }
}

impl DeferredTableFunction {
    pub(super) fn known_column_names(&self) -> Option<Vec<crate::sem::ColumnName>> {
        if self.output.has_declarations() {
            Some(self.output.names())
        } else {
            None
        }
    }
}
