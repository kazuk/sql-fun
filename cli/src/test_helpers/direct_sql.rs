pub(crate) struct StructuredFile(::tempfile::NamedTempFile);
impl StructuredFile {
    pub(crate) fn sql(format: &str) -> ::testresult::TestResult<Self> {
        let text = match format {
            "json" => r#"{"queries":["SELECT 1","SELECT 2"],"scalar":"SELECT 3"}"#,
            "toml" => "queries = ['SELECT 1', 'SELECT 2']\nscalar = 'SELECT 3'",
            "yaml" => "queries: ['SELECT 1', 'SELECT 2']\nscalar: SELECT 3",
            _ => unreachable!("test fixture format"),
        };
        Self::write(format, text)
    }
    pub(crate) fn named_cases(format: &str) -> ::testresult::TestResult<Self> {
        let text = match format {
            "json" => r#"{"cases":[{"name":"named","sql":"SELECT 1","expected":[]}]}"#,
            "toml" => "[[cases]]\nname = 'named'\nsql = 'SELECT 1'\nexpected = []",
            "yaml" => "cases:\n  - name: named\n    sql: SELECT 1\n    expected: []",
            _ => unreachable!("test fixture format"),
        };
        Self::write(format, text)
    }
    fn write(format: &str, text: &str) -> ::testresult::TestResult<Self> {
        let file = ::tempfile::NamedTempFile::with_suffix(format!(".{format}"))?;
        std::fs::write(file.path(), text)?;
        Ok(Self(file))
    }
    pub(crate) fn selection(&self, path: &str) -> String {
        format!("{}#{path}", self.0.path().display())
    }
}
