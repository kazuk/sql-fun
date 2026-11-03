mod resolve;

use crate::context::type_hint::resolve::resolve_parameter_hint;

use crate::{
    context::ParseContext,
    sem::{AnalysisError, TypeReference},
};

/// Read the explicit SQL input types installed for this query.
pub trait ParameterTypeHintRead {
    /// Look up a one-based SQL parameter number without translating it at the caller.
    /// Zero, absent hints, and numbers beyond the installed slice return `None`.
    fn get_parameter_typehint(&self, index: usize) -> Option<TypeReference>;
}

/// Install explicit SQL input types before analyzing an independent query.
///
/// SQLAST API callers can install explicit SQL input types on a mutable context
/// before calling [`crate::ast::Statement::analyze`]. Import `ParameterTypeHintWrite` and call
/// `context.setup_typehinting(&[Some("pg_catalog.int4"), None])`: the first slot
/// supplies the type of `$1`, and the second leaves `$2` for normal inference.
/// [`ParameterTypeHintRead::get_parameter_typehint`] takes the SQL parameter
/// number unchanged; zero and numbers outside the supplied slice return `None`.
/// PostgreSQL `unknown` (including its qualified spelling) leaves the slot for
/// normal inference, like `None`.
///
/// Type hints accept SQL type names for the context's explicitly selected
/// dialect, then resolve them using its catalog and search path. PostgreSQL
/// examples include `INTEGER`, `double precision`, `character varying`,
/// `timestamp with time zone`, `pg_catalog.int4`, quoted names, and arrays such
/// as `integer[2][]` or `integer ARRAY`. Quoted identifiers preserve case;
/// unquoted identifiers follow SQL case folding. `float(24)` selects `real`, and
/// `float(53)` selects `double precision`. Bare `character` and `bit` select
/// their type identities without imposing column-declaration length constraints.
/// SQLite accepts its quoted and multiword declared type names, including
/// backticks and brackets; PostgreSQL aliases are not applied to SQLite names.
/// Empty `[]` suffixes retain the existing SQLite/Turso array type identity,
/// while `[name]` quotes a name.
///
/// Parameter types describe client-protocol type identities. PostgreSQL
/// parameter OIDs carry no type modifiers, so declarations such as
/// `varchar(100)`, `numeric(10,2)`, `timestamp(2) with time zone`, and `interval
/// day to second` are rejected rather than silently losing their constraints.
/// Declaration size syntax is also rejected for SQLite hints.
///
/// Callers that only need to interpret spelling can use
/// [`crate::sem::parse_client_type`]. This shared
/// API returns a `TypeReference` without catalog lookup or search-path
/// resolution; syntactically valid user-defined names need not exist yet.
/// Canonical `FullName::from_str` and `TypeReference::from_str` retain their
/// separate, case-preserving spelling and serialization contracts.
///
/// Malformed or unresolved names return an error and preserve the complete
/// previous setup. Successful setup replaces all previous hints, including when
/// passed an empty slice or only `None` entries.
///
/// Create an independent [`crate::context::SchemaFileContext`] over the
/// shared [`crate::context::FrozenBaseContext`] catalog for each query.
/// If reusing a mutable context, explicitly install the next query's hints,
/// including an empty setup for unhinted queries. Nested queries and realized
/// CTEs share that query's finalized parameter order and hints. Hints assign SQL
/// types during name resolution; incompatible uses still produce semantic
/// diagnostics. They do not insert casts or change executable SQL. The accepted
/// spelling is independent of the named-placeholder lexer's current
/// `${name:ident}` hint syntax.
///
pub trait ParameterTypeHintWrite {
    /// Resolve zero-based slots (`hints[0]` describes `$1`) using this context's
    /// catalog and effective search path. [`crate::sem::parse_client_type`] interprets
    /// SQL type spelling using this context's dialect and rejects type modifiers.
    /// Resolve every hint before replacing state; an error preserves the previous setup completely.
    /// Empty input and all-`None` input successfully replace the previous setup.
    /// PostgreSQL `unknown` hints leave the corresponding slot unspecified.
    ///
    /// Hints belong to this query layer, never its shared frozen catalog. Set up
    /// each independent query explicitly, including queries without hints.
    /// Nested queries share the outer query's setup. This operation does not
    /// rewrite executable SQL or install casts.
    ///
    /// # Errors
    /// Returns an error for malformed type syntax or an unresolved catalog type.
    fn setup_typehinting(&mut self, hints: &[Option<&str>]) -> Result<(), AnalysisError>;
}

/// Mutable request state, deliberately excluded from the frozen schema catalog.
#[cfg_attr(test, derive(Clone))]
#[derive(Debug, Default)]
pub(crate) struct ParameterTypeHints(Vec<Option<TypeReference>>);

impl ParameterTypeHints {
    pub(crate) fn lookup(&self, index: usize) -> Option<TypeReference> {
        let slot = index.checked_sub(1)?;
        self.0.get(slot).cloned().flatten()
    }

    pub(crate) fn resolve(
        hints: &[Option<&str>],
        context: &impl ParseContext,
    ) -> Result<Self, AnalysisError> {
        let mut resolved = Vec::with_capacity(hints.len());
        for (slot, hint) in hints.iter().enumerate() {
            let ty = if let Some(hint) = hint {
                resolve_parameter_hint(slot, hint, context)?
            } else {
                None
            };
            resolved.push(ty);
        }
        Ok(Self(resolved))
    }
}

#[cfg(test)]
mod tests;
