use crate::sem::{InputTypeConstraint, SemScalarExpr, TypeReference};

/// A parameter-dependent expression whose admissible types remain SQL constraints.
/// Rust type selection is a later processing stage.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ConstrainedExpr {
    operation: String,
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
            arguments: arguments.to_vec(),
            types: types.clone(),
            not_null,
            name: name.to_owned(),
        }
    }
    /// SQL operation represented by this expression.
    pub fn operation(&self) -> &str {
        &self.operation
    }
    /// Analyzed operands, preserving numbered parameter references.
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
        for value in &mut self.arguments {
            value.walk(visitor)?;
        }
        Ok(())
    }
}

impl ConstrainedExpr {
    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        let domain = constraints.node_domain(self.node);
        if !domain.is_any() {
            self.types = domain.clone();
        }
    }
}
