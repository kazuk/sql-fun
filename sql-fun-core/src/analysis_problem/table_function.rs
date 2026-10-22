use super::DiagnosticMetadata;

/// Invalid caller-supplied table-function output definitions.
#[derive(Clone, Debug, Eq, Hash, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
#[cfg_attr(feature = "openrpc-schema", derive(::schemars::JsonSchema))]
pub enum TableFunctionProblem {
    /// A record result needs a caller-supplied shape.
    DefinitionRequired,
    /// Outer definitions conflict with ROWS FROM placement or WITH ORDINALITY.
    InvalidPlacement,
    /// The function already declares its result shape.
    DefinitionNotAllowed,
    /// A definition repeats a column name.
    DuplicateColumn(String),
    /// A declared type cannot be resolved.
    TypeNotFound(String),
    /// A declared collation cannot be uniquely resolved.
    CollationNotFound(String),
    /// More column aliases than output columns were supplied.
    TooManyAliases,
    /// Invalid declaration-local type modifiers.
    InvalidTypmod(String),
}
impl TableFunctionProblem {
    /// Invalid placement of outer record definitions.
    #[must_use]
    pub const fn invalid_placement() -> Self {
        Self::InvalidPlacement
    }
    /// Missing caller-supplied record definitions.
    #[must_use]
    pub const fn definition_required() -> Self {
        Self::DefinitionRequired
    }
    /// Definitions cannot override an existing result declaration.
    #[must_use]
    pub const fn definition_not_allowed() -> Self {
        Self::DefinitionNotAllowed
    }
    /// Duplicate name within one record definition.
    #[must_use]
    pub fn duplicate_column(name: &str) -> Self {
        Self::DuplicateColumn(name.to_owned())
    }
    /// Missing type in the current catalog/search path.
    #[must_use]
    pub fn type_not_found(name: &str) -> Self {
        Self::TypeNotFound(name.to_owned())
    }
    /// Missing or ambiguous collation in the current catalog/search path.
    #[must_use]
    pub fn collation_not_found(name: &str) -> Self {
        Self::CollationNotFound(name.to_owned())
    }
    /// Too many positional aliases.
    #[must_use]
    pub const fn too_many_aliases() -> Self {
        Self::TooManyAliases
    }
    /// Invalid type modifier arguments for the declared type.
    #[must_use]
    pub fn invalid_typmod(message: &str) -> Self {
        Self::InvalidTypmod(message.to_owned())
    }
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidTypmod(_) => "table_function_invalid_typmod",
            Self::DefinitionRequired => "table_function_definition_required",
            Self::InvalidPlacement => "table_function_definition_placement",
            Self::DefinitionNotAllowed => "table_function_definition_not_allowed",
            Self::DuplicateColumn(_) => "table_function_duplicate_column",
            Self::TypeNotFound(_) => "table_function_type_not_found",
            Self::CollationNotFound(_) => "table_function_collation_not_found",
            Self::TooManyAliases => "table_function_too_many_aliases",
        }
    }
    pub(super) fn render_metadata(&self) -> DiagnosticMetadata {
        DiagnosticMetadata::from(match self {
            Self::InvalidTypmod(message) => format!("invalid table-function column type modifier: {message}"),
            Self::InvalidPlacement => "outer column definitions require one function, no per-function definitions, and no WITH ORDINALITY".to_owned(),
            Self::DefinitionRequired => "a column definition list is required for functions returning record".to_owned(),
            Self::DefinitionNotAllowed => "a column definition list is only allowed for functions returning record without declared output columns".to_owned(),
            Self::DuplicateColumn(name) => format!("table-function column {name} specified more than once"),
            Self::TypeNotFound(name) => format!("table-function column type not found: {name}"),
            Self::CollationNotFound(name) => format!("table-function collation not uniquely resolved: {name}"),
            Self::TooManyAliases => "table-function column alias list has more entries than output columns".to_owned(),
        })
    }
}
