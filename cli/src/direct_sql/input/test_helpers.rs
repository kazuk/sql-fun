pub(super) struct StructuredFile(::tempfile::NamedTempFile);
impl StructuredFile {
    pub(super) fn new(format: &str) -> ::testresult::TestResult<Self> {
        let file = ::tempfile::NamedTempFile::with_suffix(format!(".{format}"))?;
        let text = match format {
            "json" => r#"{"queries":["SELECT 1","SELECT 2"],"scalar":"SELECT 3"}"#,
            "toml" => "queries = ['SELECT 1', 'SELECT 2']\nscalar = 'SELECT 3'",
            "yaml" => "queries: ['SELECT 1', 'SELECT 2']\nscalar: SELECT 3",
            _ => unreachable!("test fixture format"),
        };
        std::fs::write(file.path(), text)?;
        Ok(Self(file))
    }
    pub(super) fn selection(&self, path: &str) -> String {
        format!("{}#{path}", self.0.path().display())
    }
}
