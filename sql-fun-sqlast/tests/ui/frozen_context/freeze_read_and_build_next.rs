use std::path::Path;

use sql_fun_sqlast::sem::{BaseParseContext, FrozenBaseContext, ParseContext, SchemaFileContext};

fn freeze_read_and_rebuild(builder: SchemaFileContext) {
    let frozen = builder.freeze();
    let _metadata = frozen.metadata();
    let clone = frozen.clone();
    let _next_builder = SchemaFileContext::new(Path::new("schema.sql"), &clone);
}

fn assert_frozen_traits<T: Clone + std::fmt::Debug + Send + Sync>() {}

const _: fn(SchemaFileContext) = freeze_read_and_rebuild;
const _: fn() = assert_frozen_traits::<FrozenBaseContext>;

fn main() {}
