pub(crate) fn cast_catalog(
    context: &str,
    preferred: Option<bool>,
) -> Result<::tempfile::NamedTempFile, Box<dyn std::error::Error>> {
    let mut record = ::serde_json::json!({"func": "0", "method": "b", "context": context});
    if let Some(preferred) = preferred {
        record["is_prefferd"] = ::serde_json::json!(preferred);
    }
    let file = ::tempfile::NamedTempFile::new()?;
    ::serde_json::to_writer(
        file.as_file(),
        &::serde_json::json!({"source": {"target": record}}),
    )?;
    Ok(file)
}
