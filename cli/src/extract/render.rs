use ::handlebars::{
    Context, Handlebars, Helper, HelperDef, HelperResult, Output, RenderContext, RenderErrorReason,
};
use ::serde_json::Value;

struct SqlHelper {
    kind: &'static str,
}

impl HelperDef for SqlHelper {
    fn call<'reg: 'rc, 'rc>(
        &self,
        helper: &Helper<'rc>,
        _: &'reg Handlebars<'reg>,
        _: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
        output: &mut dyn Output,
    ) -> HelperResult {
        let value = helper
            .param(0)
            .and_then(|p| p.value().get(self.kind))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                RenderErrorReason::Other(format!("expected a typed SQL {}", self.kind))
            })?;
        if self.kind == "identifier" {
            output.write(&format!("\"{}\"", value.replace('"', "\"\"")))?;
        } else {
            output.write(value)?;
        }
        Ok(())
    }
}

pub(super) struct Renderer {
    engine: Handlebars<'static>,
}

impl Renderer {
    pub(super) fn new() -> Result<Self, anyhow::Error> {
        let mut engine = Handlebars::new();
        engine.set_strict_mode(true);
        // Keep the default escape policy. Only these typed helpers emit SQL verbatim.
        engine.register_helper("ident", Box::new(SqlHelper { kind: "identifier" }));
        engine.register_helper("sql", Box::new(SqlHelper { kind: "fragment" }));
        for (name, source) in [
            ("domains", include_str!("templates/domains.hbs")),
            ("tables", include_str!("templates/tables.hbs")),
            ("functions", include_str!("templates/functions.hbs")),
            ("views", include_str!("templates/views.hbs")),
        ] {
            engine.register_template_string(name, source)?;
        }
        Ok(Self { engine })
    }

    pub(super) fn render(&self, name: &str, data: &Value) -> Result<String, anyhow::Error> {
        Ok(self.engine.render(name, data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::{Renderer, fragment, identifier};

    #[test]
    fn renders_legal_quoted_names_signatures_and_expressions() {
        let renderer = Renderer::new().unwrap();
        let domain = serde_json::json!([{"schema": identifier("名\" space"), "name": identifier("DOMAIN"), "definition": fragment("text DEFAULT '<&>'::text CHECK (VALUE <> '')")}]);
        let table = serde_json::json!([{"schema": identifier("名\" space"), "table": identifier("select"), "columns": [{"column": identifier("a\"b"), "data_type": fragment("numeric(10,2)"), "not_null": true, "default_expr": fragment("(1 < 2)::int")}]}]);
        let function = serde_json::json!([{"schema": identifier("名\" space"), "name": identifier("FUNC\""), "arguments": fragment("IN \"odd arg\" integer DEFAULT 1, OUT \"OUT name\" text"), "result": fragment("text")}]);
        let sql = format!(
            "{}{}{}",
            renderer.render("domains", &domain).unwrap(),
            renderer.render("tables", &table).unwrap(),
            renderer.render("functions", &function).unwrap()
        );
        assert!(sql.contains("DEFAULT ((1 < 2)::int)"));
        assert!(sql.contains("DEFAULT '<&>'::text"));
        assert!(sql.contains("\"a\"\"b\" numeric(10,2)"));
        assert!(sql.contains("OUT \"OUT name\" text"));
        assert_eq!(pg_query::parse(&sql).unwrap().protobuf.stmts.len(), 3);
    }

    #[test]
    fn identifiers_and_server_fragments_have_distinct_contracts() {
        let renderer = Renderer::new().unwrap();
        let data = serde_json::json!([{
            "schema": identifier("odd\" schema"), "view": identifier("SELECT<&"),
            "definition": fragment("SELECT '<&>\"'::text AS \"a\";")
        }]);
        assert_eq!(
            renderer.render("views", &data).unwrap(),
            "CREATE VIEW \"odd\"\" schema\".\"SELECT<&\" AS SELECT '<&>\"'::text AS \"a\";\n\n"
        );
        let wrong = serde_json::json!([{"schema": fragment("public"), "view": identifier("v"), "definition": fragment("SELECT 1;")}]);
        assert!(renderer.render("views", &wrong).is_err());
        let wrong = serde_json::json!([{"schema": identifier("public"), "view": identifier("v"), "definition": identifier("SELECT 1;")}]);
        assert!(renderer.render("views", &wrong).is_err());
    }
}

pub(super) fn identifier(value: &str) -> Value {
    serde_json::json!({"identifier": value})
}
pub(super) fn fragment(value: &str) -> Value {
    serde_json::json!({"fragment": value})
}
