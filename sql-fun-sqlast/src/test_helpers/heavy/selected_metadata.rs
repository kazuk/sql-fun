use crate::{
    StringSpan,
    context::{BaseContext, FrozenBaseContext, MutSourcePathWithSpan, SchemaFileContext},
    sem::SourcePathWithSpan,
    test_helpers::selected_metadata::assert_selected_metadata,
};
use ::sql_fun_core::{SqlFunArgs, SqlFunMetadata};
use std::path::Path;
/// Load real builtin metadata per call to verify the selected analysis environment.
/// Initialization is repeated in every independent test process.
pub(crate) fn context(
    metadata: &SqlFunMetadata,
) -> Result<SchemaFileContext, Box<dyn std::error::Error>> {
    let args = <SqlFunArgs as ::clap::Parser>::try_parse_from([
        "sqlfun",
        "--metadata-file",
        "this-metadata-file-does-not-exist.toml",
        "--sql-fun-home",
        env!("SQL_FUN_HOME"),
        "--postgres-version",
        "17",
    ])?;
    let base = BaseContext::new(&args, metadata)?;
    assert_selected_metadata(&base, metadata);
    let mut context = {
        let analysis_base = &FrozenBaseContext::from(base);
        SchemaFileContext::new(Path::new("query.sql"), analysis_base, &args, metadata)
    };
    context.set_source_path_with_span(&SourcePathWithSpan::new(
        "query.sql",
        &StringSpan::default(),
    ));
    Ok(context)
}
