use crate::StringSpan;
use crate::sem::{
    InputTypeConstraint, ParameterConstraints, ResolvedTypes, TypeReference,
    scalar_expr::SemScalarExprNode,
};

/// A numbered SQL input with its selected application type when available.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamRef {
    position: u32,
    #[serde(skip)]
    location: Option<StringSpan>,
    constraint: ResolvedTypes,
    #[serde(skip)]
    node: Option<usize>,
}

impl ParamRef {
    pub(crate) fn new(position: u32, node: usize, constraint: &InputTypeConstraint) -> Self {
        Self {
            position,
            location: None,
            node: Some(node),
            constraint: constraint.into(),
        }
    }
    pub(crate) const fn with_location(mut self, location: Option<StringSpan>) -> Self {
        self.location = location;
        self
    }
    pub(crate) const fn location(&self) -> Option<StringSpan> {
        self.location
    }
    /// Original one-based parameter identity.
    pub const fn position(&self) -> u32 {
        self.position
    }
    /// Types used by this occurrence, narrowed to a selected application binding.
    /// [`ParameterConstraints`] retains the complete SQL-admissible input domain.
    pub const fn constraint(&self) -> &ResolvedTypes {
        &self.constraint
    }
    pub(crate) const fn node(&self) -> Option<usize> {
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
        self.node = None;
        self.location = None;
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        let Some(node) = self.node else {
            return;
        };
        self.constraint = if let Some(parameter) = constraints.parameter(self.position)
            && let Some(binding) = parameter.binding_type()
        {
            ResolvedTypes::exact(&binding)
        } else {
            constraints.node_domain(node).into()
        };
    }
}

impl ParamRef {
    pub(crate) const fn finish_inference(&mut self) {
        self.node = None;
    }
}
