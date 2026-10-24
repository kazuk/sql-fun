/// Completed schema prefix requested by an analysis client.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
pub enum SchemaLevel {
    /// Builtin catalog definitions, without builtin SQL.
    Builtin,
    /// Builtin definitions and the server's default SQL.
    Default,
    /// Default SQL followed by configured extensions.
    Extensions,
    /// Extensions followed by the selected project dump.
    #[default]
    Project,
}

impl SchemaLevel {
    /// Select builtin catalog definitions only.
    #[must_use]
    pub const fn builtin() -> Self {
        Self::Builtin
    }
    /// Select builtin definitions and default SQL.
    #[must_use]
    pub const fn default_sql() -> Self {
        Self::Default
    }
    /// Select all configured extensions as well as default SQL.
    #[must_use]
    pub const fn extensions() -> Self {
        Self::Extensions
    }
    /// Select the complete project schema.
    #[must_use]
    pub const fn project() -> Self {
        Self::Project
    }
    /// Whether this prefix includes the server's builtin SQL file.
    #[must_use]
    pub const fn includes_default(self) -> bool {
        !matches!(self, Self::Builtin)
    }
    /// Whether configured extensions belong to this prefix.
    #[must_use]
    pub const fn includes_extensions(self) -> bool {
        matches!(self, Self::Extensions | Self::Project)
    }
    /// Whether a project dump and its expected hash are required.
    #[must_use]
    pub const fn includes_project(self) -> bool {
        matches!(self, Self::Project)
    }
}
