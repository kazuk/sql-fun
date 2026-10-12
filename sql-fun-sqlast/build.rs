//! Adapt pinned parser sources with optional precise type locations.
use std::{env, fs, path::PathBuf};

fn replace_once(source: &mut String, before: &str, after: &str) {
    assert_eq!(source.matches(before).count(), 1, "upstream anchor changed");
    *source = source.replacen(before, after, 1);
}

fn main() {
    if env::var_os("CARGO_FEATURE_SQLITE").is_none()
        || env::var_os("CARGO_FEATURE_WORKAROUND_TURSO_9791").is_none()
    {
        return;
    }

    let source_path = "src/sqlite_parser/upstream/parser.rs";
    println!("cargo:rerun-if-changed={source_path}");
    println!("cargo:rerun-if-changed=src/sqlite_parser/type-spans.rs.txt");
    let mut source = fs::read_to_string(source_path).expect("pinned parser source");
    replace_once(
        &mut source,
        "pub struct Parser<'a> {\n    lexer: Lexer<'a>,",
        "pub struct Parser<'a> {\n    lexer: Lexer<'a>,\n    retain_type_spans: bool,\n    type_spans: Vec<TypeSpan>,",
    );
    replace_once(
        &mut source,
        "            lexer: Lexer::new(input),",
        "            lexer: Lexer::new(input),\n            retain_type_spans: false,\n            type_spans: Vec::new(),",
    );
    let start = source
        .find("    fn parse_type(&mut self)")
        .expect("type parser");
    let end = start
        + source[start..]
            .find("    /// Parse a parenthesized list")
            .expect("type parser end");
    let mut type_parser = source[start..end].to_owned();
    replace_once(
        &mut type_parser,
        "        let mut type_name =",
        "        let syntax_start = if self.peek()?.is_some() { self.offset() } else { 0 };\n        let mut type_name =",
    );
    replace_once(
        &mut type_parser,
        "        // Inline STRUCT/UNION types",
        "        let mut syntax_end = self.offset();\n\n        // Inline STRUCT/UNION types",
    );
    replace_once(
        &mut type_parser,
        "                    type_name.push_str(from_bytes_as_str(tok.value));",
        "                    type_name.push_str(from_bytes_as_str(tok.value));\n                    syntax_end = self.offset();",
    );
    replace_once(
        &mut type_parser,
        "                    type_name.push_str(name.as_str());",
        "                    type_name.push_str(name.as_str());\n                    syntax_end = self.offset();",
    );
    replace_once(
        &mut type_parser,
        "        // Check for array suffix(es)",
        "        if size.is_some() { syntax_end = self.offset(); }\n        if self.retain_type_spans {\n            self.type_spans.push(TypeSpan::new(&type_name, syntax_start, syntax_end));\n        }\n\n        // Check for array suffix(es)",
    );
    source.replace_range(start..end, &type_parser);
    let start = source
        .find("    fn parse_limit(&mut self)")
        .expect("limit parser");
    let end = start
        + source[start..]
            .find("        Ok(Some(Limit {")
            .expect("limit result");
    let mut limit_parser = source[start..end].to_owned();
    replace_once(
        &mut limit_parser,
        "        let expr = self.parse_expr(0)?;",
        "        let span_start = self.type_spans.len();\n        let expr = self.parse_expr(0)?;\n        let span_middle = self.type_spans.len();",
    );
    replace_once(
        &mut limit_parser,
        "                    let limit = self.parse_expr(0)?;",
        "                    let limit = self.parse_expr(0)?;\n                    self.type_spans[span_start..].rotate_left(span_middle - span_start);",
    );
    source.replace_range(start..end, &limit_parser);
    source.push_str(
        &fs::read_to_string("src/sqlite_parser/type-spans.rs.txt").expect("type-span API"),
    );
    replace_once(
        &mut source,
        "unsafe { str::from_utf8_unchecked(bytes) }",
        "str::from_utf8(bytes).expect(\"UTF-8 parser input\")",
    );
    replace_once(
        &mut source,
        "unsafe { str::from_utf8_unchecked(bytes).to_owned() }",
        "from_bytes_as_str(bytes).to_owned()",
    );
    source = source.replace("crate::", "crate::sqlite_parser::");
    source = source.replace("#[cfg(test)]", "#[cfg(any())]");
    let mut lexer = fs::read_to_string("src/sqlite_parser/upstream/lexer.rs")
        .expect("pinned lexer source")
        .replace("crate::", "crate::sqlite_parser::")
        .replace("#[cfg(test)]", "#[cfg(any())]");
    replace_once(
        &mut lexer,
        "    pub unsafe fn to_utf8_unchecked(&self) -> String {\n        String::from_utf8_unchecked(self.as_bytes().to_vec())\n    }",
        "    pub fn to_utf8_unchecked(&self) -> String {\n        String::from_utf8(self.as_bytes().to_vec()).expect(\"UTF-8 lexer token\")\n    }",
    );
    println!("cargo:rerun-if-changed=src/sqlite_parser/upstream/lexer.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    fs::write(output.join("lexer.rs"), lexer).expect("generated lexer");
    fs::write(output.join("parser.rs"), source).expect("generated parser");
}
