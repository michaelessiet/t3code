//! Typed layer over [`RpcSession`]: call/subscribe by generated
//! [`RpcMethod`](vitre_contracts::support::RpcMethod) marker types from
//! `vitre_contracts::methods`, so payloads and results are checked against the
//! contracts instead of hand-built `serde_json::Value`s.
//!
//! Decode policy: the generated unions all carry `Unknown` fallback variants,
//! so a value that fails to decode indicates real contract drift — it fails
//! the call/stream loudly rather than being skipped.

use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use vitre_contracts::support::RpcMethod;

use crate::envelope::CauseEncoded;
use crate::error::RpcError;
use crate::session::{RpcSession, StreamEvent, Subscription};

/// Failure of a typed RPC.
#[derive(Debug, thiserror::Error)]
pub enum TypedError<E> {
    /// The handler failed with its contract-declared error union (an
    /// `Exit.Failure` whose cause is typed `Fail` entries).
    #[error("rpc failed with a contract error")]
    Failed(E),
    /// Transport-, protocol- or defect-level failure.
    #[error(transparent)]
    Rpc(RpcError),
}

impl<E> TypedError<E> {
    /// True when reconnecting could help (transport loss, defect poison) as
    /// opposed to a typed contract error the server would repeat.
    pub fn is_transport(&self) -> bool {
        matches!(
            self,
            TypedError::Rpc(RpcError::Ws(_) | RpcError::ConnectionClosed | RpcError::Transport(_))
        )
    }
}

/// Split an [`RpcError`] into the contract error union vs everything else.
/// Mirrors the TS client's "expected failure" test: every cause entry must be
/// a typed `Fail` (`client.ts` `hasOnlyExpectedFailures`).
fn classify<E: DeserializeOwned>(error: RpcError) -> TypedError<E> {
    let RpcError::Failed { cause } = error else {
        return TypedError::Rpc(error);
    };
    let only_fails =
        !cause.is_empty() && cause.iter().all(|c| matches!(c, CauseEncoded::Fail { .. }));
    if !only_fails {
        return TypedError::Rpc(RpcError::Failed { cause });
    }
    let Some(CauseEncoded::Fail { error }) = cause.into_iter().next() else {
        unreachable!("cause verified non-empty and all Fail");
    };
    match serde_json::from_value::<E>(error) {
        Ok(typed) => TypedError::Failed(typed),
        Err(error) => TypedError::Rpc(RpcError::Json(error)),
    }
}

impl RpcSession {
    /// Typed unary request.
    pub async fn call_typed<M: RpcMethod>(
        &self,
        payload: &M::Payload,
    ) -> Result<M::Success, TypedError<M::Error>> {
        let payload = serde_json::to_value(payload)
            .map_err(|error| TypedError::Rpc(RpcError::Json(error)))?;
        match self.call(M::TAG, payload).await {
            Ok(value) => serde_json::from_value(value)
                .map_err(|error| TypedError::Rpc(RpcError::Json(error))),
            Err(error) => Err(classify::<M::Error>(error)),
        }
    }

    /// Typed streaming request. Same Ack contract as [`RpcSession::subscribe`]:
    /// after each [`TypedStreamEvent::Values`] the caller must
    /// [`TypedSubscription::ack`] or the server pauses the stream.
    pub fn subscribe_typed<M: RpcMethod>(
        &self,
        payload: &M::Payload,
    ) -> Result<TypedSubscription<M>, RpcError> {
        debug_assert!(M::STREAM, "{} is not a streaming rpc", M::TAG);
        let payload = serde_json::to_value(payload)?;
        Ok(TypedSubscription {
            inner: self.subscribe(M::TAG, payload)?,
            _method: PhantomData,
        })
    }
}

#[derive(Debug)]
pub enum TypedStreamEvent<M: RpcMethod> {
    /// One `Chunk` batch. The server will not send another until Acked.
    Values(Vec<M::Success>),
    /// Terminal `Exit` for the subscription.
    Completed(Result<(), TypedError<M::Error>>),
}

pub struct TypedSubscription<M: RpcMethod> {
    inner: Subscription,
    _method: PhantomData<fn() -> M>,
}

impl<M: RpcMethod> TypedSubscription<M> {
    pub async fn next(&mut self) -> Option<TypedStreamEvent<M>> {
        self.inner.next().await.map(decode_event::<M>)
    }

    pub fn ack(&self) -> Result<(), RpcError> {
        self.inner.ack()
    }

    pub fn interrupt(&self) -> Result<(), RpcError> {
        self.inner.interrupt()
    }
}

fn decode_event<M: RpcMethod>(event: StreamEvent) -> TypedStreamEvent<M> {
    match event {
        StreamEvent::Values(values) => {
            let mut decoded = Vec::with_capacity(values.len());
            for value in values {
                match serde_json::from_value::<M::Success>(value) {
                    Ok(item) => decoded.push(item),
                    // Contract drift the Unknown fallbacks could not absorb —
                    // poison the stream instead of silently dropping data.
                    Err(error) => {
                        return TypedStreamEvent::Completed(Err(TypedError::Rpc(RpcError::Json(
                            error,
                        ))));
                    }
                }
            }
            TypedStreamEvent::Values(decoded)
        }
        // The terminal Success value of a stream is void; only failure matters.
        StreamEvent::Completed(result) => {
            TypedStreamEvent::Completed(result.map(|_| ()).map_err(classify::<M::Error>))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use vitre_contracts::methods::{OrchestrationSubscribeShellError, ServerGetConfigError};

    fn failed(cause: serde_json::Value) -> RpcError {
        RpcError::Failed {
            cause: serde_json::from_value(cause).expect("cause decodes"),
        }
    }

    #[test]
    fn classify_decodes_a_typed_fail() {
        let error = failed(json!([
            {"_tag": "Fail", "error": {
                "_tag": "EnvironmentAuthorizationError",
                "message": "missing scope",
                "requiredScope": "orchestration:read",
            }}
        ]));
        match classify::<ServerGetConfigError>(error) {
            TypedError::Failed(ServerGetConfigError::EnvironmentAuthorizationError(inner)) => {
                assert_eq!(
                    inner.required_scope,
                    vitre_contracts::AuthEnvironmentScope::OrchestrationRead
                );
            }
            other => panic!("expected typed failure, got {other:?}"),
        }
    }

    #[test]
    fn classify_routes_unrecognized_typed_errors_to_the_unknown_variant() {
        let error = failed(json!([
            {"_tag": "Fail", "error": {"_tag": "SomeFutureError", "detail": 42}}
        ]));
        match classify::<OrchestrationSubscribeShellError>(error) {
            TypedError::Failed(OrchestrationSubscribeShellError::Unknown(value)) => {
                assert_eq!(value["_tag"], "SomeFutureError");
            }
            other => panic!("expected Unknown typed failure, got {other:?}"),
        }
    }

    #[test]
    fn classify_keeps_defects_as_rpc_errors() {
        let error = failed(json!([
            {"_tag": "Die", "defect": {"message": "handler crashed"}}
        ]));
        assert!(matches!(
            classify::<ServerGetConfigError>(error),
            TypedError::Rpc(RpcError::Failed { .. })
        ));
    }

    #[test]
    fn classify_keeps_mixed_causes_as_rpc_errors() {
        let error = failed(json!([
            {"_tag": "Fail", "error": {"_tag": "X"}},
            {"_tag": "Interrupt", "fiberId": 1},
        ]));
        assert!(matches!(
            classify::<ServerGetConfigError>(error),
            TypedError::Rpc(RpcError::Failed { .. })
        ));
    }

    #[test]
    fn transport_errors_are_flagged_for_reconnect() {
        let transport: TypedError<ServerGetConfigError> =
            TypedError::Rpc(RpcError::Transport("connection closed".into()));
        assert!(transport.is_transport());
        let typed = classify::<ServerGetConfigError>(failed(json!([
            {"_tag": "Fail", "error": {"_tag": "SomeFutureError"}}
        ])));
        assert!(!typed.is_transport());
    }
}
