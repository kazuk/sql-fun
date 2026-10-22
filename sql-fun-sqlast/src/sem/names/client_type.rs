mod postgres;
#[cfg(feature = "sqlite")]
mod sqlite;
mod token;

use self::token::TypeTokens;
use crate::ast::Dialect;
use crate::sem::{AnalysisError, ParseContext, TypeReference};

/// Interpret one SQL type name for a client-protocol parameter hint.
///
/// The context's explicitly selected dialect controls SQL quoting, case folding,
/// aliases, and array syntax. The result retains named identity only: parsing
/// neither looks up the catalog nor applies a search path. Callers can resolve
/// it with their context's `canonicalize_type_ref` afterwards.
///
/// PostgreSQL type modifiers cannot be represented by protocol parameter OIDs
/// and are rejected, including interval field restrictions. `float(p)` selects
/// `float4` or `float8` and is accepted. Bare `character` and `bit` select their
/// type identity without imposing the length used by SQL column declarations.
/// SQLite declarations retain their logical name, without PostgreSQL aliases.
/// This SQL entry point does not change canonical `FromStr` or serde formats.
///
/// # Errors
/// Returns an error for malformed or trailing input, unsupported type modifiers,
/// or SQLite syntax when the `sqlite` feature is unavailable. A syntactically
/// valid name can succeed even when it does not exist in the catalog.
pub fn parse_client_type(
    input: &str,
    context: &impl ParseContext,
) -> Result<TypeReference, AnalysisError> {
    if input.contains('\0') {
        return Err(invalid("NUL in type name"));
    }
    let dialect = context.dialect();
    if dialect == Dialect::postgres() {
        let tokens = TypeTokens::postgres(input)?;
        self::postgres::parse(&tokens)
    } else {
        #[cfg(feature = "sqlite")]
        {
            let tokens = TypeTokens::sqlite(input)?;
            self::sqlite::parse(&tokens)
        }
        #[cfg(not(feature = "sqlite"))]
        Err(invalid("SQLite requires the sqlite feature"))
    }
}

#[cfg(test)]
mod tests;

fn invalid(detail: &str) -> AnalysisError {
    AnalysisError::unexpected_input_error(&format!("SQL type name: {detail}"))
}

fn typmod() -> AnalysisError {
    invalid("type modifiers are not representable in client parameter types")
}
