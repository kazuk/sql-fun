use crate::{names::OperatorName, sem::OperatorDefinition};

/// get operator definition
pub trait OperatorInfoRead {
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
