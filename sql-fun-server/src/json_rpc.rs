use sql_fun_server_api::SqlFunServerApiError;

#[derive(serde::Deserialize)]
pub struct JsonRpcRequest {
    #[serde(rename = "jsonrpc")]
    json_rpc_version: String,
    id: serde_json::Value,
    method: String,
    params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn method(&self) -> &str {
        &self.method
    }

    pub fn params(&self) -> Option<&serde_json::Value> {
        self.params.as_ref()
    }
}

#[derive(serde::Serialize)]
pub struct JsonRpcResponse {
    #[serde(rename = "jsonrpc")]
    json_rpc_version: String,
    id: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(request: &JsonRpcRequest, value: serde_json::Value) -> Self {
        Self {
            json_rpc_version: String::from("2.0"),
            id: Some(request.id.clone()),
            result: Some(value),
            error: None,
        }
    }

    pub fn error(request: &JsonRpcRequest, error: &JsonRpcError) -> Self {
        Self {
            json_rpc_version: String::from("2.0"),
            id: Some(request.id.clone()),
            result: None,
            error: Some(error.clone()),
        }
    }
}

#[derive(serde::Serialize, Clone)]
pub struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

impl From<serde_json::Error> for JsonRpcError {
    fn from(_value: serde_json::Error) -> Self {
        Self::json_deserialize_error()
    }
}

impl From<SqlFunServerApiError> for JsonRpcError {
    fn from(value: SqlFunServerApiError) -> Self {
        Self::server_api_execution_error(value)
    }
}

impl JsonRpcError {
    /// error code for parameter missing
    // TODO: Fixme value
    const CODE_PARAMETER_REQUIRED: i32 = 0;

    /// error code for unknown method
    // TODO: Fixme value
    const CODE_UNKNOWN_METHOD: i32 = 0;

    /// error code for json deserialize error
    // TODO: Fixme value
    const CODE_JSON_DESERIALIZE_ERROR: i32 = 0;

    /// error code for internal server error
    // TODO: Fixme value
    const CODE_INTERNAL_SERVER_ERROR: i32 = 0;

    #[must_use]
    pub fn parameter_required(method: &str) -> Self {
        Self {
            code: Self::CODE_PARAMETER_REQUIRED,
            message: format!("required params for method : {method}"),
            data: None,
        }
    }

    #[must_use]
    pub fn unknown_method(method: &str) -> Self {
        Self {
            code: Self::CODE_UNKNOWN_METHOD,
            message: format!("unknown method : {method}"),
            data: None,
        }
    }

    #[must_use]
    pub fn json_deserialize_error() -> Self {
        Self {
            code: Self::CODE_JSON_DESERIALIZE_ERROR,
            message: "unexpected params".to_string(),
            data: None,
        }
    }

    #[must_use]
    pub fn server_api_execution_error(_err: SqlFunServerApiError) -> Self {
        Self {
            code: Self::CODE_INTERNAL_SERVER_ERROR,
            message: "internal server error".to_string(),
            data: None,
        }
    }
}
