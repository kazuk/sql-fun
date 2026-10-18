use crate::sem::scalar_expr::SemScalarExprNode;
use crate::sem::{InputTypeConstraint, TypeReference};

/// A numbered SQL input, independent of application hints and Rust type mapping.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamRef {
    position: u32,
    constraint: InputTypeConstraint,
    #[serde(skip)]
    node: usize,
}

impl ParamRef {
    pub(crate) fn new(position: u32, node: usize, constraint: &InputTypeConstraint) -> Self {
        Self {
            position,
            node,
            constraint: constraint.clone(),
        }
    }
    /// Original one-based parameter identity.
    pub const fn position(&self) -> u32 {
        self.position
    }
    /// SQL-derived admissible input types; `any` is a successful inference result.
    pub const fn constraint(&self) -> &InputTypeConstraint {
        &self.constraint
    }
    pub(crate) const fn node(&self) -> usize {
        self.node
    }
}

impl SemScalarExprNode for ParamRef {
    fn get_type(&self) -> Option<TypeReference> {
        self.constraint.single_type()
    }
    fn is_not_null(&self) -> Option<bool> {
        None
    }
}

impl ParamRef {
    pub(super) const fn canonicalize_grouping_key(&mut self) {
        self.node = 0;
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &crate::sem::ParameterConstraints) {
        self.constraint = constraints.node_domain(self.node).clone();
    }
}
