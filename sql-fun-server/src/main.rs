#[tokio::main]
async fn main() -> Result<(), sql_fun_server::ServerError> {
    sql_fun_server::service().await?;
    Ok(())
}
