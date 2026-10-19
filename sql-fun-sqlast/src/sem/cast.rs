/// type cast definition
#[derive(Debug, Clone)]
pub struct CastDefinition {
    cast_context: CastContext,
}

/// type cast context
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastContext {
    /// implicit and prefeerd
    ImplicitPrefeerd,
    /// implicit
    Implicit,
    /// assignment
    Assignment,
    /// explicit
    Explicit,
    /// non-conversion (same type)
    NoConversion,
}

impl CastDefinition {
    /// create with context
    #[must_use]
    pub const fn new(cc: CastContext) -> Self {
        Self { cast_context: cc }
    }

    /// is explicit
    #[must_use]
    pub const fn use_in_explicit(&self) -> bool {
        self.cast_context.use_in_explicit()
    }

    /// is implicit
    #[must_use]
    pub const fn use_in_implicit(&self) -> bool {
        self.cast_context.use_in_implicit()
    }

    /// get context
    #[must_use]
    pub const fn context(&self) -> CastContext {
        self.cast_context
    }

    /// in implicit and preferred
    #[must_use]
    pub const fn is_implicit_preferred(&self) -> bool {
        self.cast_context.is_implicit_preferred()
    }

    /// is no conversion
    #[must_use]
    pub const fn is_no_conversion(&self) -> bool {
        self.cast_context.is_no_conversion()
    }
}

impl CastContext {
    /// Construct a preferred implicit context for semantic test fixtures.
    #[cfg(test)]
    pub(crate) const fn implicit_preferred() -> Self {
        Self::ImplicitPrefeerd
    }

    /// Construct an identity context for semantic test fixtures.
    #[cfg(test)]
    pub(crate) const fn no_conversion() -> Self {
        Self::NoConversion
    }

    /// Construct an implicit context for semantic test fixtures.
    #[cfg(test)]
    pub(crate) const fn implicit() -> Self {
        Self::Implicit
    }

    /// Construct a preferred implicit context for semantic test fixtures.
    #[cfg(test)]
    pub(crate) const fn implicit_preferred() -> Self {
        Self::ImplicitPrefeerd
    }

    /// Construct an explicit cast context.
    pub(crate) const fn explicit() -> Self {
        Self::Explicit
    }

    /// Construct an assignment context for semantic test fixtures.
    #[cfg(test)]
    pub(crate) const fn assignment() -> Self {
        Self::Assignment
    }

    /// cast is defined, always use explicit
    #[must_use]
    pub const fn use_in_explicit(&self) -> bool {
        true
    }

    /// use in implicit
    #[must_use]
    pub const fn use_in_implicit(&self) -> bool {
        matches!(
            self,
            Self::Implicit | Self::ImplicitPrefeerd | Self::NoConversion
        )
    }

    /// use in assignment context
    #[must_use]
    pub const fn use_in_assignment(&self) -> bool {
        matches!(
            self,
            Self::Implicit | Self::ImplicitPrefeerd | Self::Assignment | Self::NoConversion
        )
    }

    /// Higher score means better match in overload resolution.
    #[must_use]
    pub const fn overload_priority_score(&self) -> i32 {
        match self {
            Self::NoConversion => 10,
            Self::ImplicitPrefeerd => 9,
            Self::Implicit => 5,
            // can not to use in overload resolution
            _ => 0,
        }
    }

    /// type cast is implicit
    #[must_use]
    pub const fn is_implicit(&self) -> bool {
        matches!(self, Self::Implicit | Self::ImplicitPrefeerd)
    }

    /// test cast is implicit and prefeered
    #[must_use]
    pub const fn is_implicit_preferred(&self) -> bool {
        matches!(self, Self::ImplicitPrefeerd)
    }

    /// test cast is non-conversion
    #[must_use]
    pub const fn is_no_conversion(&self) -> bool {
        matches!(self, Self::NoConversion)
    }
}
