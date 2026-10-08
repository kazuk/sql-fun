use sql_fun_sqlast::sem::FrozenBaseContext;

fn access_inner_arc(context: FrozenBaseContext) {
    let _inner = context.0;
}

fn main() {}
