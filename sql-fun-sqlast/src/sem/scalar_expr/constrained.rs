use crate::sem::{InputTypeConstraint, SemScalarExpr, TypeReference};

/// A parameter-dependent expression whose admissible types remain SQL constraints.
/// Rust type selection is a later processing stage.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConstrainedExpr {
    operation: String,
    #[serde(default)]
    operator_binding: Option<Box<crate::sem::ResolvedOperator>>,
    #[serde(default)]
    cast_binding: Option<crate::sem::QueryNameResolution>,
    #[serde(default)]
    semantic_call: Option<Box<crate::sem::scalar_expr::func_call::FunctionBinding>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    indirection: Option<Box<crate::sem::IndirectionExpr>>,
    arguments: Vec<SemScalarExpr>,
    types: InputTypeConstraint,
    not_null: Option<bool>,
    name: String,
    #[serde(skip)]
    node: usize,
}

impl ConstrainedExpr {
    pub(crate) fn new(
        node: usize,
        operation: &str,
        arguments: &[SemScalarExpr],
        types: &InputTypeConstraint,
        not_null: Option<bool>,
        name: &str,
    ) -> Self {
        Self {
            node,
            operation: operation.to_owned(),
            operator_binding: None,
            cast_binding: None,
            semantic_call: None,
            indirection: None,
            arguments: arguments.to_vec(),
            types: types.clone(),
            not_null,
            name: name.to_owned(),
        }
    }
    pub(crate) fn retain_indirection(&mut self, value: &crate::sem::IndirectionExpr) {
        self.indirection = Some(Box::new(value.clone()));
    }
    /// Structured array or composite access retained during parameter inference.
    pub fn indirection(&self) -> Option<&crate::sem::IndirectionExpr> {
        self.indirection.as_deref()
    }
    pub(crate) fn retain_semantic_call(&mut self, call: &crate::sem::FuncCallExpr) {
        self.semantic_call = Some(Box::new(call.binding_snapshot()));
    }
    /// Original function binding and modifiers, when this represents a function.
    pub fn function_binding(&self) -> Option<&crate::sem::scalar_expr::func_call::FunctionBinding> {
        self.semantic_call.as_deref()
    }
    pub(crate) fn retain_cast_binding(&mut self, binding: &crate::sem::QueryNameResolution) {
        self.cast_binding = Some(binding.clone());
    }
    /// Target-name resolution for a parameter-dependent cast.
    pub const fn cast_binding(&self) -> Option<&crate::sem::QueryNameResolution> {
        self.cast_binding.as_ref()
    }
    pub(crate) fn retain_operator_binding(&mut self, binding: &crate::sem::ResolvedOperator) {
        self.operator_binding = Some(Box::new(binding.clone()));
    }
    /// Completed operator lookup for a parameter-dependent operator expression.
    pub fn operator_binding(&self) -> Option<&crate::sem::ResolvedOperator> {
        self.operator_binding.as_deref()
    }
    /// SQL operation represented by this expression.
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// Direct operation operands, preserving numbered parameter references.
    /// Structured access operands are retained by [`Self::indirection`].
    pub fn arguments(&self) -> &[SemScalarExpr] {
        &self.arguments
    }
    /// Admissible output types, retaining alternatives without picking a Rust type.
    pub const fn types(&self) -> &InputTypeConstraint {
        &self.types
    }
    pub(crate) const fn node(&self) -> usize {
        self.node
    }
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn single_type(&self) -> Option<TypeReference> {
        self.types.single_type()
    }
    pub(crate) const fn is_not_null(&self) -> Option<bool> {
        self.not_null
    }
}

impl ConstrainedExpr {
    pub(crate) fn walk<V: crate::sem::traversal::SemanticVisitor>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), crate::sem::AnalysisError> {
        if let Some(indirection) = &mut self.indirection {
            indirection.walk(visitor)?;
        }
        if let Some(call) = &mut self.semantic_call {
            call.walk(visitor)?;
        }
        for value in &mut self.arguments {
            value.walk(visitor)?;
        }
        Ok(())
    }
}

impl ConstrainedExpr {
    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.node = 0;
        if let Some(binding) = &mut self.operator_binding {
            binding.canonicalize_grouping_key();
        }
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        let domain = constraints.node_domain(self.node);
        if !domain.is_any() {
            self.types = domain.clone();
        }
        if let Some(indirection) = &mut self.indirection {
            indirection.set_result_type(&domain.single_type());
        }
    }
}
