use sql_fun_sqlast::sem::{FrozenBaseContext, ParseContext};

fn mutate_frozen_context(mut context: FrozenBaseContext) {
    context.set_search_path(&[]);
}

fn main() {}
