//! Effect-RPC WebSocket client for the T3 Code sidecar.
//!
//! The sidecar (`apps/server`) speaks effect `unstable/rpc` over a WebSocket at
//! `/ws`, serialized as plain JSON — one `_tag`-discriminated envelope per text
//! frame (`RpcSerialization.layerJson`). The envelope vocabulary is defined in
//! `effect/unstable/rpc/RpcMessage.ts`; [`envelope`] mirrors its
//! transport-encoded (`*Encoded`) shapes exactly.
//!
//! Streaming back pressure: after every `Chunk` the server pauses the stream
//! until the client sends `Ack{requestId}`. A client that forgets Acks receives
//! exactly one chunk and hangs.

pub mod envelope;
pub mod error;
pub mod http;
pub mod session;
pub mod sidecar;

pub use error::RpcError;
pub use http::EnvironmentHttp;
pub use session::{RpcSession, StreamEvent, Subscription};
pub use sidecar::{Sidecar, SidecarConfig};
