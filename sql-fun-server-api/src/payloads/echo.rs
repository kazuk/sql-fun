/// Object or array parameters returned unchanged by the diagnostic echo method.
#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EchoArgs(EchoPayload);

impl EchoArgs {
    /// Consume the arguments and return their original JSON value.
    #[must_use]
    pub fn into_value(self) -> ::serde_json::Value {
        self.0.into_value()
    }
}

#[cfg_attr(feature = "openrpc-schema", derive(schemars::JsonSchema))]
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
enum EchoPayload {
    Object(::serde_json::Map<String, ::serde_json::Value>),
    Array(Vec<::serde_json::Value>),
}

impl EchoPayload {
    fn into_value(self) -> ::serde_json::Value {
        match self {
            Self::Object(value) => ::serde_json::Value::Object(value),
            Self::Array(value) => ::serde_json::Value::Array(value),
        }
    }
}
