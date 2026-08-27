//! Transport-encoded RPC envelopes, mirroring `FromClientEncoded` /
//! `FromServerEncoded` in `effect/unstable/rpc/RpcMessage.ts`. Request ids are
//! stringified integers on the wire (the server rehydrates them with
//! `BigInt(id)`); ids must round-trip exactly or responses get misrouted.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::RpcError;

/// Client → server messages.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "_tag")]
pub enum FromClient<'a> {
    Request {
        id: String,
        tag: &'a str,
        payload: Value,
        /// Wire shape is an array of `[name, value]` pairs, not an object.
        headers: Vec<(String, String)>,
    },
    Ack {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    Interrupt {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    Ping,
    Eof,
}

/// Server → client messages.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "_tag")]
pub enum FromServer {
    Chunk {
        #[serde(rename = "requestId")]
        request_id: String,
        values: Vec<Value>,
    },
    Exit {
        #[serde(rename = "requestId")]
        request_id: String,
        exit: ExitEncoded,
    },
    /// Connection-level defect; poisons every in-flight request.
    Defect {
        defect: Value,
    },
    Pong,
    ClientProtocolError {
        error: Value,
    },
    /// Forward compatibility with envelope kinds newer sidecars may add.
    #[serde(other)]
    Unknown,
}

/// `ExitEncoded<A, E>`: terminal outcome of one request.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum ExitEncoded {
    Success { value: Value },
    Failure { cause: Vec<CauseEncoded> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "_tag")]
pub enum CauseEncoded {
    /// Typed RPC error (the `error` union declared in the contract).
    Fail { error: Value },
    /// Untyped defect (unexpected server crash inside the handler).
    Die { defect: Value },
    Interrupt {
        #[serde(rename = "fiberId", default)]
        fiber_id: Option<i64>,
    },
}

impl ExitEncoded {
    pub fn into_result(self) -> Result<Value, RpcError> {
        match self {
            ExitEncoded::Success { value } => Ok(value),
            ExitEncoded::Failure { cause } => Err(RpcError::Failed { cause }),
        }
    }
}
