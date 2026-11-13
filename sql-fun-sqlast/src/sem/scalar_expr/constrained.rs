use crate::sem::{
    AnalysisError, FuncCallExpr, IndirectionExpr, InputTypeConstraint, ParameterConstraints,
    ParseContext, QueryNameResolution, ResolvedOperator, SemScalarExpr, TypeReference,
    scalar_expr::func_call::FunctionBinding,
    traversal::{SemanticVisitor, SemanticVisitorMut},
};

/// A parameter-dependent expression whose admissible types remain SQL constraints.
/// Rust type selection is a later processing stage.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConstrainedExpr {
    operation: String,
    #[serde(default)]
    operator_binding: Option<Box<ResolvedOperator>>,
    #[serde(skip)]
    binding_validation_pending: bool,
    #[serde(default)]
    cast_binding: Option<QueryNameResolution>,
    #[serde(default)]
    semantic_call: Option<Box<FunctionBinding>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    indirection: Option<Box<IndirectionExpr>>,
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
            binding_validation_pending: false,
            cast_binding: None,
            semantic_call: None,
            indirection: None,
            arguments: arguments.to_vec(),
            types: types.clone(),
            not_null,
            name: name.to_owned(),
        }
    }
    pub(crate) fn retain_indirection(&mut self, value: &IndirectionExpr) {
        self.name = value.projection_name().to_owned();
        self.indirection = Some(Box::new(value.clone()));
    }
    /// Structured array or composite access retained during parameter inference.
    pub fn indirection(&self) -> Option<&IndirectionExpr> {
        self.indirection.as_deref()
    }
    pub(crate) fn retain_semantic_call(&mut self, call: &FuncCallExpr) {
        self.name = call.name().local_name();
        self.semantic_call = Some(Box::new(call.binding_snapshot()));
    }
    /// Original function binding and modifiers, when this represents a function.
    pub fn function_binding(&self) -> Option<&FunctionBinding> {
        self.semantic_call.as_deref()
    }
    pub(crate) fn retain_cast_binding(&mut self, binding: &QueryNameResolution) {
        self.cast_binding = Some(binding.clone());
    }
    /// Target-name resolution for a parameter-dependent cast.
    pub const fn cast_binding(&self) -> Option<&QueryNameResolution> {
        self.cast_binding.as_ref()
    }
    pub(crate) fn retain_operator_binding(&mut self, binding: &ResolvedOperator) {
        self.binding_validation_pending = self
            .arguments
            .iter()
            .any(SemScalarExpr::needs_type_inference);
        self.operator_binding = Some(Box::new(binding.clone()));
    }
    /// Completed operator lookup for a parameter-dependent operator expression.
    pub fn operator_binding(&self) -> Option<&ResolvedOperator> {
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
    pub(crate) fn walk<V: SemanticVisitorMut>(
        &mut self,
        visitor: &mut V,
    ) -> Result<(), AnalysisError> {
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
    pub(crate) fn visit<V: SemanticVisitor>(&self, visitor: &mut V) -> Result<(), AnalysisError> {
        if let Some(binding) = &self.cast_binding {
            binding.visit_type(visitor)?;
        }
        if let Some(indirection) = &self.indirection {
            indirection.visit(visitor)?;
        }
        if let Some(call) = &self.semantic_call {
            call.visit(visitor)?;
        }
        for value in &self.arguments {
            value.visit(visitor)?;
        }
        Ok(())
    }
}

impl ConstrainedExpr {
    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.node = 0;
        self.binding_validation_pending = false;
        if let Some(binding) = &mut self.operator_binding {
            binding.canonicalize_grouping_key();
        }
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        let domain = constraints.node_domain(self.node);
        if !domain.is_any() {
            self.types = domain.clone();
        }
        if let Some(indirection) = &mut self.indirection {
            indirection.set_result_type(&domain.single_type());
        }
    }
}

impl ConstrainedExpr {
    pub(crate) fn function_call(&self) -> Option<FuncCallExpr> {
        self.semantic_call
            .as_ref()
            .map(|binding| binding.invocation(&self.arguments))
    }
    pub(crate) fn finalize_call(
        &mut self,
        context: &mut impl ParseContext,
    ) -> Result<(), AnalysisError> {
        if let Some(binding) = &mut self.semantic_call {
            binding.finalize(context, &mut self.arguments, &mut self.not_null)?;
        }
        if self.binding_validation_pending
            && let Some(binding) = &self.operator_binding
            && let Some(types) = binding.validate_application_inputs(&self.arguments, context)?
        {
            self.types = types;
            self.binding_validation_pending = false;
        }
        Ok(())
    }
}
