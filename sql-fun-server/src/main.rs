//! sql-fun-server

fn main() -> Result<(), sql_fun_server::ServerError> {
    sql_fun_server::install_server_panic_hook();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(sql_fun_server::service())
}
