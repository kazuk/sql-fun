use crate::ast::Dialect;
use crate::context::FrozenBaseContext;
use crate::test_helpers::TestParseContext;

pub(crate) fn base_with_public_schema(dialect: Dialect) -> FrozenBaseContext {
    use crate::{context::ParseContext, names::SchemaName};

    let mut base = TestParseContext::default().with_dialect(dialect);
    base.set_get_search_path_result(&[SchemaName::from("public")]);
    base.freeze()
}

pub(crate) fn schema_file(sql: &str) -> ::tempfile::NamedTempFile {
    let file = ::tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), sql).unwrap();
    file
}
