use crate::context::PgBuiltInFunctions;

pub(crate) fn functions_with_scalar_and_output_overloads() -> PgBuiltInFunctions {
    let prototypes: ::serde_json::Value = ::serde_json::from_str(
        r#"[
            {"args":["text"],"ret":"text","is_strict":true},
            {"args":["text","any"],"ret":"text","is_strict":false},
            {"args":["bool"],"ret":"bool","is_strict":true},
            {"args":["int4"],"ret":"record","is_strict":true},
            {"args":["int8"],"ret":"record","is_strict":true}
        ]"#,
    )
    .unwrap();
    let details: ::serde_json::Value = ::serde_json::from_str(
        r#"[
            {"name":"probe","allargtypes":"{text}","argmodes":"{i}","argnames":"{value}"},
            {"name":"probe","allargtypes":"{text,any}","argmodes":"{i,v}","argnames":"{pattern,values}"},
            {"name":"probe","allargtypes":"{int4,text,int4}","argmodes":"{i,o,o}","argnames":"{value,label,count}"},
            {"name":"probe","allargtypes":"{int8,int8}","argmodes":"{i,o}","argnames":"{value,total}"}
        ]"#,
    ).unwrap();
    let directory = ::tempfile::tempdir().unwrap();
    let functions = directory.path().join("functions.json");
    let detail_path = directory.path().join("details.json");
    let mut overloads = prototypes.as_array().unwrap().clone();
    for overload in &mut overloads {
        overload["name"] = ::serde_json::json!("probe");
    }
    std::fs::write(
        &functions,
        ::serde_json::to_vec(&::serde_json::json!([
            {"name": "probe", "overloads": overloads}
        ]))
        .unwrap(),
    )
    .unwrap();
    std::fs::write(&detail_path, ::serde_json::to_vec(&details).unwrap()).unwrap();
    PgBuiltInFunctions::load_test_catalog(&functions, &detail_path)
}
