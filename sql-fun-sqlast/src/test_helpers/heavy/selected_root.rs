use crate::context::{BaseContext, FrozenBaseContext};
use ::sql_fun_core::SqlFunArgs;
use ::sql_fun_core::SqlFunMetadataCollection;
use std::path::Path;

/// Load real builtin metadata per call to verify the selected analysis environment.
/// Initialization is repeated in every independent test process.
pub(crate) fn selected_root() -> Result<FrozenBaseContext, Box<dyn std::error::Error>> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/selected-postgres.toml");
    let mut metadata = SqlFunMetadataCollection::load_from(fixture)?;
    let selected = metadata.remove("analytics").expect("selected metadata");
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        "this-metadata-file-does-not-exist.toml",
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "17",
    ])?;
    Ok(BaseContext::new(&args, &selected)?.into())
}
