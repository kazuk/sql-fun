use crate::analysis_problem::{DiagnosticMessage, DiagnosticMetadata};

/// An invalid `PostgreSQL` serial-family column declaration.
#[derive(Clone, Debug, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum SerialProblem {
    /// Serial arrays cannot be declared. Diagnostic code: `serial_array`.
    Array,
    /// The underlying integer type does not accept modifiers. Diagnostic code: `serial_modifier`.
    Modifier,
    /// Explicit NULL conflicts with implicit NOT NULL. Diagnostic code: `serial_nullable`.
    Nullable,
    /// An explicit default conflicts with the sequence default. Diagnostic code: `serial_default_conflict`.
    Default,
    /// Identity conflicts with the sequence default. Diagnostic code: `serial_identity_conflict`.
    Identity,
    /// A generated expression conflicts with the sequence default. Diagnostic code: `serial_generated_conflict`.
    Generated,
}
impl SerialProblem {
    /// Reject a serial array.
    #[must_use]
    pub const fn array() -> Self {
        Self::Array
    }
    /// Reject a serial type modifier.
    #[must_use]
    pub const fn modifier() -> Self {
        Self::Modifier
    }
    /// Reject explicit NULL.
    #[must_use]
    pub const fn nullable() -> Self {
        Self::Nullable
    }
    /// Reject a second default.
    #[must_use]
    pub const fn default_conflict() -> Self {
        Self::Default
    }
    /// Reject identity with serial.
    #[must_use]
    pub const fn identity() -> Self {
        Self::Identity
    }
    /// Reject a generated expression with serial.
    #[must_use]
    pub const fn generated() -> Self {
        Self::Generated
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::Array => "serial_array",
            Self::Modifier => "serial_modifier",
            Self::Nullable => "serial_nullable",
            Self::Default => "serial_default_conflict",
            Self::Identity => "serial_identity_conflict",
            Self::Generated => "serial_generated_conflict",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(self.diagnostic_message())
    }
}
impl DiagnosticMessage for SerialProblem {
    fn diagnostic_message(&self) -> String {
        match self {
            Self::Array => "array of serial is not implemented by PostgreSQL",
            Self::Modifier => "type modifier is not allowed for a serial integer column",
            Self::Nullable => "NULL conflicts with serial's implicit NOT NULL",
            Self::Default => "DEFAULT conflicts with serial's sequence default",
            Self::Identity => "identity conflicts with serial's sequence default",
            Self::Generated => "generated expression conflicts with serial's sequence default",
        }
        .to_owned()
    }
}
