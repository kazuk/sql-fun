use crate::test_helpers::heavy;
use ::sql_fun_sqlast::context::{BaseContext, FrozenBaseContext, SchemaFileContext};

/// Loads the full builtin function/operator/cast metadata once per call.
/// This is intentionally heavy to validate the real catalog contracts.
pub(crate) fn context() -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let (args, selected) = heavy::environment("17")?;
    let base = BaseContext::new(&args, &selected)?;
    Ok(SchemaFileContext::new(
        std::path::Path::new("query.sql"),
        &FrozenBaseContext::from(base),
        &args,
        &selected,
    ))
}
