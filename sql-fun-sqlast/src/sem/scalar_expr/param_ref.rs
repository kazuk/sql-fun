use crate::StringSpan;
use crate::sem::{
    InputTypeConstraint, ParameterConstraints, TypeReference, scalar_expr::SemScalarExprNode,
};

/// A numbered SQL input with its selected application type when available.
#[derive(Debug, Clone, Eq, Hash, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ParamRef {
    position: u32,
    #[serde(skip)]
    location: Option<StringSpan>,
    constraint: InputTypeConstraint,
    // Recollection must not feed finalized application types into SQL inputs.
    #[serde(skip)]
    input_constraint: Option<InputTypeConstraint>,
    #[serde(skip)]
    node: usize,
}

impl ParamRef {
    pub(crate) fn new(position: u32, node: usize, constraint: &InputTypeConstraint) -> Self {
        Self {
            position,
            location: None,
            node,
            constraint: constraint.clone(),
            input_constraint: None,
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
    pub const fn constraint(&self) -> &InputTypeConstraint {
        &self.constraint
    }
    pub(crate) fn input_constraint(&self) -> &InputTypeConstraint {
        self.input_constraint.as_ref().unwrap_or(&self.constraint)
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
    pub(super) fn canonicalize_grouping_key(&mut self) {
        self.node = 0;
        self.location = None;
        self.input_constraint = None;
    }
    pub(crate) fn apply_constraints(&mut self, constraints: &ParameterConstraints) {
        self.input_constraint = Some(constraints.sql_node_domain(self.node).clone());
        self.constraint = if let Some(parameter) = constraints.parameter(self.position)
            && let Some(binding) = parameter.binding_type()
        {
            InputTypeConstraint::exact(&binding)
        } else {
            constraints.node_domain(self.node).clone()
        };
    }
}
