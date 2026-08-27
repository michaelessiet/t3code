//! One WebSocket RPC session: a writer task, a reader task that routes
//! server envelopes to per-request channels, unary calls, and Ack-paced
//! streaming subscriptions.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;

use crate::envelope::{ExitEncoded, FromClient, FromServer};
use crate::error::RpcError;

#[derive(Debug)]
enum ServerEvent {
    Chunk(Vec<Value>),
    Exit(ExitEncoded),
    ConnectionLost(String),
}

type Router = Arc<Mutex<HashMap<u64, mpsc::UnboundedSender<ServerEvent>>>>;

pub struct RpcSession {
    out_tx: mpsc::UnboundedSender<Message>,
    router: Router,
    pong_rx: watch::Receiver<u64>,
    next_id: AtomicU64,
}

impl RpcSession {
    pub async fn connect(ws_url: &str) -> Result<Self, RpcError> {
        let (ws, _response) = tokio_tungstenite::connect_async(ws_url).await?;
        let (mut sink, mut stream) = ws.split();

        let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();
        tokio::spawn(async move {
            while let Some(message) = out_rx.recv().await {
                if sink.send(message).await.is_err() {
                    break;
                }
            }
        });

        let router: Router = Arc::new(Mutex::new(HashMap::new()));
        let (pong_tx, pong_rx) = watch::channel(0u64);
        let reader_router = router.clone();
        tokio::spawn(async move {
            while let Some(frame) = stream.next().await {
                let text = match frame {
                    Ok(Message::Text(text)) => text,
                    Ok(Message::Close(_)) | Err(_) => break,
                    Ok(_) => continue,
                };
                let message: FromServer = match serde_json::from_str(&text) {
                    Ok(message) => message,
                    Err(error) => {
                        eprintln!("[vitre-rpc] undecodable server frame ({error}): {text}");
                        continue;
                    }
                };
                match message {
                    FromServer::Chunk { request_id, values } => {
                        route(&reader_router, &request_id, ServerEvent::Chunk(values));
                    }
                    FromServer::Exit { request_id, exit } => {
                        route(&reader_router, &request_id, ServerEvent::Exit(exit));
                    }
                    FromServer::Pong => pong_tx.send_modify(|count| *count += 1),
                    FromServer::Defect { defect } => {
                        poison(&reader_router, format!("server defect: {defect}"));
                    }
                    FromServer::ClientProtocolError { error } => {
                        poison(&reader_router, format!("client protocol error: {error}"));
                    }
                    FromServer::Unknown => {}
                }
            }
            poison(&reader_router, "connection closed".to_string());
        });

        Ok(Self {
            out_tx,
            router,
            pong_rx,
            next_id: AtomicU64::new(1),
        })
    }

    fn send(&self, message: &FromClient<'_>) -> Result<(), RpcError> {
        let json = serde_json::to_string(message)?;
        self.out_tx
            .send(Message::Text(json.into()))
            .map_err(|_| RpcError::ConnectionClosed)
    }

    fn register(&self) -> (u64, mpsc::UnboundedReceiver<ServerEvent>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::unbounded_channel();
        self.router.lock().unwrap().insert(id, tx);
        (id, rx)
    }

    fn unregister(&self, id: u64) {
        self.router.lock().unwrap().remove(&id);
    }

    /// Unary request: resolves on `Exit`.
    pub async fn call(&self, tag: &str, payload: Value) -> Result<Value, RpcError> {
        let (id, mut rx) = self.register();
        let sent = self.send(&FromClient::Request {
            id: id.to_string(),
            tag,
            payload,
            headers: Vec::new(),
        });
        if let Err(error) = sent {
            self.unregister(id);
            return Err(error);
        }
        let result = loop {
            match rx.recv().await {
                // A unary RPC shouldn't chunk, but Ack anyway so a
                // mis-declared streaming handler can still reach its Exit.
                Some(ServerEvent::Chunk(_)) => {
                    let _ = self.send(&FromClient::Ack {
                        request_id: id.to_string(),
                    });
                }
                Some(ServerEvent::Exit(exit)) => break exit.into_result(),
                Some(ServerEvent::ConnectionLost(reason)) => {
                    break Err(RpcError::Transport(reason));
                }
                None => break Err(RpcError::ConnectionClosed),
            }
        };
        self.unregister(id);
        result
    }

    /// Streaming request. The subscription does NOT auto-Ack: after each
    /// [`StreamEvent::Values`] the caller must invoke [`Subscription::ack`]
    /// or the server pauses the stream.
    pub fn subscribe(&self, tag: &str, payload: Value) -> Result<Subscription, RpcError> {
        let (id, rx) = self.register();
        let sent = self.send(&FromClient::Request {
            id: id.to_string(),
            tag,
            payload,
            headers: Vec::new(),
        });
        if let Err(error) = sent {
            self.unregister(id);
            return Err(error);
        }
        Ok(Subscription {
            request_id: id,
            rx,
            out_tx: self.out_tx.clone(),
            router: self.router.clone(),
        })
    }

    /// Application-level keepalive: send `Ping`, await the next `Pong`.
    pub async fn ping(&self, timeout: Duration) -> Result<Duration, RpcError> {
        let mut pong_rx = self.pong_rx.clone();
        pong_rx.mark_unchanged();
        let started = Instant::now();
        self.send(&FromClient::Ping)?;
        tokio::time::timeout(timeout, pong_rx.changed())
            .await
            .map_err(|_| RpcError::Transport("pong timeout".into()))?
            .map_err(|_| RpcError::ConnectionClosed)?;
        Ok(started.elapsed())
    }
}

#[derive(Debug)]
pub enum StreamEvent {
    /// One `Chunk` batch. The server will not send another until Acked.
    Values(Vec<Value>),
    /// Terminal `Exit` for the subscription.
    Completed(Result<Value, RpcError>),
}

pub struct Subscription {
    request_id: u64,
    rx: mpsc::UnboundedReceiver<ServerEvent>,
    out_tx: mpsc::UnboundedSender<Message>,
    router: Router,
}

impl Subscription {
    pub async fn next(&mut self) -> Option<StreamEvent> {
        self.rx.recv().await.map(StreamEvent::from)
    }

    /// Non-blocking receive, for asserting that the server is actually
    /// pausing between un-Acked chunks.
    pub fn try_next(&mut self) -> Option<StreamEvent> {
        self.rx.try_recv().ok().map(StreamEvent::from)
    }

    pub fn ack(&self) -> Result<(), RpcError> {
        self.send(&FromClient::Ack {
            request_id: self.request_id.to_string(),
        })
    }

    pub fn interrupt(&self) -> Result<(), RpcError> {
        self.send(&FromClient::Interrupt {
            request_id: self.request_id.to_string(),
        })
    }

    fn send(&self, message: &FromClient<'_>) -> Result<(), RpcError> {
        let json = serde_json::to_string(message)?;
        self.out_tx
            .send(Message::Text(json.into()))
            .map_err(|_| RpcError::ConnectionClosed)
    }
}

impl From<ServerEvent> for StreamEvent {
    fn from(event: ServerEvent) -> Self {
        match event {
            ServerEvent::Chunk(values) => StreamEvent::Values(values),
            ServerEvent::Exit(exit) => StreamEvent::Completed(exit.into_result()),
            ServerEvent::ConnectionLost(reason) => {
                StreamEvent::Completed(Err(RpcError::Transport(reason)))
            }
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.router.lock().unwrap().remove(&self.request_id);
    }
}

fn route(router: &Router, request_id: &str, event: ServerEvent) {
    let Ok(id) = request_id.parse::<u64>() else {
        eprintln!("[vitre-rpc] non-numeric requestId from server: {request_id}");
        return;
    };
    let sender = router.lock().unwrap().get(&id).cloned();
    match sender {
        Some(sender) => {
            let _ = sender.send(event);
        }
        None => eprintln!("[vitre-rpc] response for unknown request {id}"),
    }
}

fn poison(router: &Router, reason: String) {
    let senders: Vec<_> = router.lock().unwrap().drain().map(|(_, tx)| tx).collect();
    for sender in senders {
        let _ = sender.send(ServerEvent::ConnectionLost(reason.clone()));
    }
}
