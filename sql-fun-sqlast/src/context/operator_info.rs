use crate::{
    names::{FullName, OperatorName},
    sem::{CascadeRemoval, OperatorDefinition, OperatorIdentity, OperatorLinks},
};

/// get operator definition
pub trait OperatorInfoRead {
    /// Registered removals for dependents of this exact operator overload.
    fn operator_removals(
        &self,
        _identity: &OperatorIdentity,
    ) -> Vec<(FullName, Box<dyn CascadeRemoval>)> {
        Vec::new()
    }

    /// Whether the builtin catalog requires this exact operator.
    fn operator_is_system_required(&self, _identity: &OperatorIdentity) -> bool {
        false
    }

    /// Read declaration links for an exact signature, including an operator shell.
    fn operator_links(&self, _identity: &OperatorIdentity) -> Option<&OperatorLinks> {
        None
    }

    /// A catalog guarantee that the operator returns a non-null value when
    /// both operands are non-null. Strictness alone does not provide this.
    /// `None` means no guarantee is available, including custom implementations.
    fn operator_result_is_not_null(&self, _name: &OperatorName) -> Option<bool> {
        None
    }

    /// Read an exact catalog identity, without search-path expansion.
    /// Use `BaseParseContext::get_operator` to resolve SQL references.
    ///
    /// `has_left`, `has_right` both true then returns Binary operator.
    /// one of `has_left`, `has_right` is true then returns Left unary or Right unary operator.
    ///
    fn get_operator_op(
        &self,
        name: &OperatorName,
        has_left: bool,
        has_right: bool,
    ) -> Option<&OperatorDefinition>;
}
