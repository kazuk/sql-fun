use std::path::Path;

use sql_fun_sqlast::sem::SchemaFileContext;

fn use_mutable_context_as_base(builder: SchemaFileContext) {
    let _next_builder = SchemaFileContext::new(Path::new("schema.sql"), &builder);
}

fn main() {}
