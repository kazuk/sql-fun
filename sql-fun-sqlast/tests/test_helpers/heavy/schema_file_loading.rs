use crate::test_helpers::schema_file_loading::environment;
use ::sql_fun_sqlast::context::{BaseContext, FrozenBaseContext};
/// Load the real catalog per call to check selected schema-loader metadata.
/// No catalog initialization is shared across test processes.
pub(crate) fn selected_frozen_base(
    fixture: &str,
) -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    let (args, selected) = environment(fixture)?;
    Ok(BaseContext::new(&args, &selected)?.into())
}
