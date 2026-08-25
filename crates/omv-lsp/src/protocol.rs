use serde::{Deserialize, Serialize};
use serde_json::Value;

/// JSON-RPC 2.0 envelope. One struct covers requests, responses, and
/// notifications; which fields are present tells them apart.
#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ResponseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl Message {
    pub fn request(id: i64, method: &str, params: Value) -> Self {
        Message {
            jsonrpc: "2.0".into(),
            id: Some(Value::from(id)),
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn notification(method: &str, params: Value) -> Self {
        Message {
            jsonrpc: "2.0".into(),
            id: None,
            method: Some(method.into()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    /// A reply to a server→client request. Servers block on some of these
    /// (`client/registerCapability`), so they must always be answered.
    pub fn response(id: Value, result: Value) -> Self {
        Message {
            jsonrpc: "2.0".into(),
            id: Some(id),
            method: None,
            params: None,
            result: Some(result),
            error: None,
        }
    }

    pub fn is_response(&self) -> bool {
        self.method.is_none() && self.id.is_some()
    }
}

/// Serialise with the `Content-Length` header the protocol requires.
pub fn encode(message: &Message) -> Result<Vec<u8>, serde_json::Error> {
    let body = serde_json::to_vec(message)?;
    let mut out = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    out.extend_from_slice(&body);
    Ok(out)
}
