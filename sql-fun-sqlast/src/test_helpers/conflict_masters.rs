use crate::{context::ConflictMasters, test_helpers::TestParseContext};
use ::serde_json::Value;

pub(crate) fn base_without_masters(
    dialect: crate::ast::Dialect,
) -> Result<crate::context::BaseContext, Box<dyn std::error::Error>> {
    let directory = ::tempfile::tempdir()?;
    for file in ["pg_type.json", "functions.json", "func_detail.json"] {
        std::fs::write(directory.path().join(file), "[]")?;
    }
    for file in [
        "binary_op.json",
        "left_unary_op.json",
        "right_unary_op.json",
        "casts.json",
    ] {
        std::fs::write(directory.path().join(file), "{}")?;
    }
    std::fs::write(
        directory.path().join("binary_op.json"),
        r#"{"+": [{"left":"int4", "right":"int4", "result":"int4", "strict":true}]}"#,
    )?;
    let args = ::sql_fun_core::SqlFunArgs::builder(&std::collections::HashMap::new())?
        .builtin_schema_dir(directory.path())
        .build()?;
    let metadata = TestParseContext::default().with_dialect(dialect);
    Ok(crate::context::BaseContext::new(
        &args,
        crate::context::BaseParseContext::metadata(&metadata),
    )?)
}

pub(crate) struct Masters {
    directory: ::tempfile::TempDir,
    collations: Value,
    classes: Value,
    defect: Option<String>,
}

impl Masters {
    pub(crate) fn new() -> Self {
        let collation = |name: &str, provider: &str| {
            ::serde_json::json!({"schema":"pg_catalog", "name": name, "encoding":-1,
            "provider":provider, "deterministic":true, "collate":null, "ctype":null, "locale":null, "icu_rules":null, "version":null})
        };
        let class = |method: &str| {
            ::serde_json::json!({"schema":"public", "name":"ops", "access_method":method,
            "family":{"schema":"outside", "name":"family", "access_method":method},
            "input_type":{"schema":"outside", "name":"a_type"}, "default":true, "key_type":null})
        };
        Self {
            directory: ::tempfile::tempdir().unwrap(),
            collations: ::serde_json::json!({"database":{"encoding":6,"encoding_name":"UTF8","provider":"c","collate":"C","ctype":"C", "locale":null,"icu_rules":null,"version":null,
                "default_collation":{"schema":"pg_catalog","name":"default","encoding":-1}},
                "collations":[collation("default", "d"), collation("C", "c")]}),
            classes: ::serde_json::json!([class("btree"), class("hash")]),
            defect: None,
        }
    }
    pub(crate) fn set_encoding(&mut self, encoding: i32) {
        self.collations["database"]["encoding"] = encoding.into();
    }
    pub(crate) fn add_collation(
        &mut self,
        schema: &str,
        name: &str,
        encoding: i32,
        provider: &str,
    ) {
        let mut value = self.collations["collations"][1].clone();
        value["schema"] = schema.into();
        value["name"] = name.into();
        value["encoding"] = encoding.into();
        value["provider"] = provider.into();
        self.collations["collations"]
            .as_array_mut()
            .unwrap()
            .push(value);
    }
    pub(crate) fn corrupt(&mut self, defect: &str) {
        match defect {
            "duplicate_collation" => {
                let value = self.collations["collations"][0].clone();
                self.collations["collations"]
                    .as_array_mut()
                    .unwrap()
                    .push(value);
            }
            "duplicate_class" => {
                let value = self.classes[0].clone();
                self.classes.as_array_mut().unwrap().push(value);
            }
            "empty_classes" => self.classes = ::serde_json::json!([]),
            "encoding" => self.collations["database"]["encoding"] = 35.into(),
            "default_identity" => {
                self.collations["database"]["default_collation"]["name"] = "absent".into()
            }
            "default_provider" => self.collations["collations"][0]["provider"] = "c".into(),
            "family_method" => self.classes[0]["family"]["access_method"] = "gin".into(),
            "malformed" | "missing" => self.defect = Some(defect.to_owned()),
            _ => panic!("unknown fixture defect"),
        }
    }
    pub(crate) fn load(&self) -> Result<ConflictMasters, crate::sem::AnalysisError> {
        let collations = self.directory.path().join("collations.json");
        let classes = self.directory.path().join("operator_classes.json");
        std::fs::write(&collations, self.collations.to_string()).unwrap();
        std::fs::write(&classes, self.classes.to_string()).unwrap();
        if self.defect.as_deref() == Some("malformed") {
            std::fs::write(&classes, "[").unwrap();
        }
        if self.defect.as_deref() == Some("missing") {
            std::fs::remove_file(&collations).unwrap();
        }
        ConflictMasters::load(&collations, &classes)
    }
    pub(crate) fn context(&self) -> TestParseContext {
        let mut context = TestParseContext::default();
        context.set_conflict_masters(&self.load().unwrap());
        crate::context::ParseContext::set_search_path(
            &mut context,
            &[crate::names::SchemaName::from("public")],
        );
        context
    }
}
