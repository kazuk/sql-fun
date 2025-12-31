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
    pub fn new(cc: CastContext) -> Self {
        Self { cast_context: cc }
    }

    /// is explicit
    pub fn use_in_explicit(&self) -> bool {
        self.cast_context.use_in_explicit()
    }

    /// is implicit
    pub fn use_in_implicit(&self) -> bool {
        self.cast_context.use_in_implicit()
    }

    /// get context
    pub fn context(&self) -> CastContext {
        self.cast_context
    }

    /// in implicit and preferred
    pub fn is_implicit_preferred(&self) -> bool {
        self.cast_context.is_implicit_preferred()
    }

    /// is no conversion
    pub fn is_no_coversion(&self) -> bool {
        self.cast_context.is_no_coversion()
    }
}

impl CastContext {
    /// cast is defined, always use explicit
    pub fn use_in_explicit(&self) -> bool {
        true
    }

    /// use in implicit
    pub fn use_in_implicit(&self) -> bool {
        matches!(
            self,
            Self::Implicit | Self::ImplicitPrefeerd | Self::NoConversion
        )
    }

    /// use in assignment context
    pub fn use_in_assignment(&self) -> bool {
        matches!(
            self,
            Self::Implicit | Self::ImplicitPrefeerd | Self::Assignment | Self::NoConversion
        )
    }

    /// Higher score means better match in overload resolution.
    pub fn overload_priority_score(&self) -> i32 {
        match self {
            Self::NoConversion => 10,
            Self::ImplicitPrefeerd => 9,
            Self::Implicit => 5,
            // can not to use in overload resolution
            _ => 0,
        }
    }

    /// type cast is implicit
    pub fn is_implicit(&self) -> bool {
        matches!(self, Self::Implicit | Self::ImplicitPrefeerd)
    }

    /// test cast is implicit and prefeered
    pub fn is_implicit_preferred(&self) -> bool {
        matches!(self, Self::ImplicitPrefeerd)
    }

    /// test cast is non-conversion
    pub fn is_no_coversion(&self) -> bool {
        matches!(self, Self::NoConversion)
    }
}
