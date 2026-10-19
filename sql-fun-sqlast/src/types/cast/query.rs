//! Explicit outcomes for cast questions that may depend on polymorphic binding.

use crate::ast::Dialect;
use crate::types::{CastContext, TypeDefinition, TypeReference};

/// A cast query with a determinate answer or an owned binding-dependent question.
#[derive(Debug, Clone)]
pub enum CastQueryOutcome<T> {
    /// A complete answer, including known absence or an empty candidate set.
    Resolved(T),
    /// Binding is required; this is neither absence nor a candidate type.
    Deferred(DeferredCastQuery),
}
impl<T> CastQueryOutcome<T> {
    /// Wrap a complete answer.
    pub const fn resolved(value: T) -> Self {
        Self::Resolved(value)
    }
    /// Retain an unevaluated question.
    pub fn deferred(query: &DeferredCastQuery) -> Self {
        Self::Deferred(query.clone())
    }
    /// Inspect a complete answer, or `None` if it remains deferred.
    pub const fn resolution(&self) -> Option<&T> {
        match self {
            Self::Resolved(value) => Some(value),
            Self::Deferred(_) => None,
        }
    }
    /// Consume a complete answer, or `None` if it remains deferred.
    pub fn into_resolution(self) -> Option<T> {
        match self {
            Self::Resolved(value) => Some(value),
            Self::Deferred(_) => None,
        }
    }
    /// Inspect the unresolved question without choosing an answer.
    pub const fn deferred_query(&self) -> Option<&DeferredCastQuery> {
        match self {
            Self::Resolved(_) => None,
            Self::Deferred(query) => Some(query),
        }
    }
}

/// Query direction, distinct from implicit/assignment/explicit eligibility.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum CastQueryDirection {
    /// A cast between two endpoints.
    Pair,
    /// All accepted sources of a target.
    Sources,
    /// All accepted implicit targets of a source.
    Targets,
}
impl CastQueryDirection {
    /// A directed endpoint pair.
    pub const fn pair() -> Self {
        Self::Pair
    }
    /// Source enumeration.
    pub const fn sources() -> Self {
        Self::Sources
    }
    /// Target enumeration.
    pub const fn targets() -> Self {
        Self::Targets
    }
}

/// An owned cast question preserving complete endpoints and signature linkage.
///
/// Only the public query implementation creates these values. A deferred query
/// does not execute binding, select a common type, or discover a catalog.
#[derive(Debug, Clone)]
pub struct DeferredCastQuery {
    direction: CastQueryDirection,
    mode: CastContext,
    dialect: Dialect,
    source: Option<TypeReference>,
    target: Option<TypeReference>,
}
impl DeferredCastQuery {
    pub(super) fn pair(
        source: &dyn TypeDefinition,
        target: &dyn TypeDefinition,
        mode: CastContext,
        dialect: Dialect,
    ) -> Self {
        Self {
            direction: CastQueryDirection::pair(),
            mode,
            dialect,
            source: Some(source.clone_box()),
            target: Some(target.clone_box()),
        }
    }
    pub(super) fn sources(
        target: &dyn TypeDefinition,
        mode: CastContext,
        dialect: Dialect,
    ) -> Self {
        Self {
            direction: CastQueryDirection::sources(),
            mode,
            dialect,
            source: None,
            target: Some(target.clone_box()),
        }
    }
    pub(super) fn targets(source: &dyn TypeDefinition, dialect: Dialect) -> Self {
        Self {
            direction: CastQueryDirection::targets(),
            mode: CastContext::implicit(),
            dialect,
            source: Some(source.clone_box()),
            target: None,
        }
    }
    /// Pair lookup or enumeration direction.
    pub const fn direction(&self) -> CastQueryDirection {
        self.direction
    }
    /// Requested cast eligibility.
    pub const fn mode(&self) -> CastContext {
        self.mode
    }
    /// The explicitly selected provider dialect.
    pub const fn dialect(&self) -> Dialect {
        self.dialect
    }
    /// The source for a pair lookup or target enumeration.
    pub fn source(&self) -> Option<&dyn TypeDefinition> {
        self.source.as_deref()
    }
    /// The target for a pair lookup or source enumeration.
    pub fn target(&self) -> Option<&dyn TypeDefinition> {
        self.target.as_deref()
    }
}

/// Operational errors, separate from unresolved eligibility and known absence.
#[derive(Debug, Clone, Eq, PartialEq, ::thiserror::Error)]
pub enum CastQueryError<E> {
    /// An actual provider failure.
    #[error("cast provider failed: {0}")]
    Provider(E),
    /// PostgreSQL constraints are not interpreted under another dialect.
    #[error("PostgreSQL polymorphic cast query is unsupported for dialect {0:?}")]
    UnsupportedDialect(Dialect),
}
impl<E> CastQueryError<E> {
    /// Preserve a provider failure.
    pub const fn provider(error: E) -> Self {
        Self::Provider(error)
    }
    /// Report an unsupported operation, without invalidating the type value.
    pub const fn unsupported_dialect(dialect: Dialect) -> Self {
        Self::UnsupportedDialect(dialect)
    }
    /// Inspect the original provider failure.
    pub const fn provider_error(&self) -> Option<&E> {
        match self {
            Self::Provider(error) => Some(error),
            Self::UnsupportedDialect(_) => None,
        }
    }
    /// Inspect an unsupported query dialect.
    pub const fn unsupported_query_dialect(&self) -> Option<Dialect> {
        match self {
            Self::UnsupportedDialect(dialect) => Some(*dialect),
            Self::Provider(_) => None,
        }
    }
}
