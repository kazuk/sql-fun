use sql_fun_sqlast::sem::{ParseContext, SchemaFileContext};

fn reuse_after_freeze(mut builder: SchemaFileContext) {
    let _frozen = builder.freeze();
    builder.set_search_path(&[]);
}

fn main() {}
