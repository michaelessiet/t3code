use serde_json::Value;

use crate::envelope::CauseEncoded;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("websocket: {0}")]
    Ws(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("http status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("connection closed")]
    ConnectionClosed,
    #[error("transport failure: {0}")]
    Transport(String),
    #[error("rpc exit failure: {}", summarize_cause(cause))]
    Failed { cause: Vec<CauseEncoded> },
}

fn summarize_cause(cause: &[CauseEncoded]) -> String {
    cause
        .iter()
        .map(|entry| match entry {
            CauseEncoded::Fail { error } => format!("Fail({})", compact(error)),
            CauseEncoded::Die { defect } => format!("Die({})", compact(defect)),
            CauseEncoded::Interrupt { fiber_id } => format!("Interrupt(fiber {fiber_id:?})"),
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn compact(value: &Value) -> String {
    let text = value.to_string();
    if text.len() > 400 {
        format!("{}…", &text[..400])
    } else {
        text
    }
}
