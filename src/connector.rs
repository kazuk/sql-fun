use sql_fun_core::SqlFunMetadata;

use crate::SqlFunError;

pub enum Connector {
    TokioPostgres,
}

impl Connector {
    pub fn get_connector(metadata: &SqlFunMetadata) -> Result<Self, SqlFunError> {
        let Some(connector) = metadata.connector() else {
            Err(SqlFunError::custom("connector not defined"))?
        };
        if connector == "tokio-postgres" {
            return Ok(Self::TokioPostgres);
        }

        Err(SqlFunError::custom("Unsupported database connector"))
    }
}
