use std::fmt;
/// Severity of a semantic diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum DiagnosticSeverity {
    /// A semantic error.
    Error,
    /// A semantic warning.
    Warning,
    /// Informational advice.
    Advice,
}
/// Statement-relative byte range retained by analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct StringSpan {
    offset: usize,
    len: usize,
}
impl StringSpan {
    /// Retain a byte range.
    #[must_use]
    pub const fn new(offset: usize, len: usize) -> Self {
        Self { offset, len }
    }
    /// Byte offset.
    #[must_use]
    pub const fn offset(&self) -> usize {
        self.offset
    }
    /// Byte length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }
    /// Whether the range is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}
impl From<&Self> for StringSpan {
    fn from(v: &Self) -> Self {
        *v
    }
}
/// A label attached to a retained source range.
#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    message: String,
    span: StringSpan,
}
impl DiagnosticLabel {
    /// Label text.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Source byte range.
    #[must_use]
    pub const fn span(&self) -> StringSpan {
        self.span
    }
}
/// Text and source labels consumed by presentation adapters.
#[derive(Debug, Clone)]
pub struct DiagnosticMetadata {
    message: String,
    labels: Vec<DiagnosticLabel>,
}
impl DiagnosticMetadata {
    /// Attach a source label.
    pub(super) fn with_label(mut self, message: &str, span: StringSpan) -> Self {
        self.labels.push(DiagnosticLabel {
            message: message.to_owned(),
            span,
        });
        self
    }
    /// Diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
    /// Recorded labels.
    #[must_use]
    pub fn labels(&self) -> &[DiagnosticLabel] {
        &self.labels
    }
}
impl From<String> for DiagnosticMetadata {
    fn from(message: String) -> Self {
        Self {
            message,
            labels: Vec::new(),
        }
    }
}
impl From<&str> for DiagnosticMetadata {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}
pub(super) trait Problem {
    const CODE: &'static str;
    const MESSAGE: &'static str;
    const SPAN_LABEL: &'static str;
    fn get_problem_span(&self) -> StringSpan;
    fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(Self::MESSAGE)
            .with_label(Self::SPAN_LABEL, self.get_problem_span())
    }
}
// Each payload keeps a distinct type while sharing the string ownership contract.
macro_rules! diagnostic_text {
    ($name:ident) => {
        #[doc = concat!("Retained diagnostic text for `", stringify!($name), "`.")]
        #[derive(Clone, PartialEq, Eq, Hash, ::serde::Serialize, ::serde::Deserialize)]
        #[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
        pub struct $name(String);

        impl $name {
            /// Retain text without owning a semantic AST value.
            #[must_use]
            pub fn new(value: &str) -> Self {
                Self(value.to_owned())
            }
            /// Retained text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl From<&Self> for $name {
            fn from(value: &Self) -> Self {
                value.clone()
            }
        }
        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }
    };
}

diagnostic_text!(FullName);
diagnostic_text!(ColumnName);
diagnostic_text!(TableName);
diagnostic_text!(TypeReference);
diagnostic_text!(TypeDefinition);
diagnostic_text!(DefElemKey);
diagnostic_text!(OperatorName);
diagnostic_text!(AExprKind);
diagnostic_text!(AliasName);
diagnostic_text!(CteName);
diagnostic_text!(ViewName);
diagnostic_text!(RuleIdentity);
diagnostic_text!(TriggerIdentity);
diagnostic_text!(OwnershipIdentity);
diagnostic_text!(Kind);
diagnostic_text!(TypeName);
diagnostic_text!(DebugName);

/// A terminal-independent piece of a diagnostic message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticMessageFragment<'a> {
    /// Ordinary prose or an untyped value.
    Text(String),
    /// A qualified name.
    FullName(&'a FullName),
    /// A column name.
    ColumnName(&'a ColumnName),
    /// A table name.
    TableName(&'a TableName),
    /// An alias.
    AliasName(&'a AliasName),
    /// A common table expression name.
    CteName(&'a CteName),
    /// A view name.
    ViewName(&'a ViewName),
    /// A type name.
    TypeName(&'a TypeName),
    /// A retained diagnostic name.
    DebugName(&'a DebugName),
}

impl DiagnosticMessageFragment<'_> {
    /// Text displayed for this fragment without terminal styling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Text(value) => value,
            Self::FullName(value) => value.as_str(),
            Self::ColumnName(value) => value.as_str(),
            Self::TableName(value) => value.as_str(),
            Self::AliasName(value) => value.as_str(),
            Self::CteName(value) => value.as_str(),
            Self::ViewName(value) => value.as_str(),
            Self::TypeName(value) => value.as_str(),
            Self::DebugName(value) => value.as_str(),
        }
    }
}

impl From<String> for DiagnosticMessageFragment<'_> {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for DiagnosticMessageFragment<'_> {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

macro_rules! name_fragment {
    ($($name:ident),* $(,)?) => {$ (
        impl<'a> From<&'a $name> for DiagnosticMessageFragment<'a> {
            fn from(value: &'a $name) -> Self {
                Self::$name(value)
            }
        }
    )*};
}
name_fragment!(
    FullName, ColumnName, TableName, AliasName, CteName, ViewName, TypeName, DebugName
);

/// Diagnostic text for an ownership lookup outcome.
#[derive(Debug, Clone, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub struct OwnershipLookup {
    code: String,
    message: String,
}
impl OwnershipLookup {
    /// Preserve the outcome classification and its message.
    #[must_use]
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.to_owned(),
            message: message.to_owned(),
        }
    }
    /// Stable outcome classification.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Outcome text.
    #[must_use]
    pub fn diagnostic_message(&self) -> String {
        self.message.clone()
    }
}
impl From<&Self> for OwnershipLookup {
    fn from(v: &Self) -> Self {
        v.clone()
    }
}

impl From<&String> for CteName {
    fn from(value: &String) -> Self {
        Self::new(value)
    }
}
