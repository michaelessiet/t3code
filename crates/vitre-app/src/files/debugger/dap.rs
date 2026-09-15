//! Bounded stdio Debug Adapter Protocol transport. Only explicitly launched
//! adapter processes are owned; dropping the last connection stops its child.
use anyhow::{Context as _, Result, anyhow, bail};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    process::Stdio,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    process::Command,
    sync::{broadcast, mpsc, oneshot},
};

const MAX_FRAME: usize = 8 * 1024 * 1024;

async fn write_frame(writer: &mut (impl AsyncWrite + Unpin), value: &Value) -> Result<()> {
    let body = serde_json::to_vec(value)?;
    anyhow::ensure!(body.len() <= MAX_FRAME, "DAP request too large");
    tokio::time::timeout(Duration::from_secs(3), async {
        writer
            .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
            .await?;
        writer.write_all(&body).await?;
        writer.flush().await
    })
    .await
    .context("Debug adapter stopped reading requests")??;
    Ok(())
}

async fn read_frame(reader: &mut (impl AsyncBufRead + Unpin)) -> Result<Value> {
    let mut header = Vec::new();
    let mut content_length = None;
    loop {
        let mut line = Vec::new();
        // Bound headers as well as bodies. A malicious/noisy adapter must
        // not be able to grow an unbounded read_line allocation.
        loop {
            let byte = reader.read_u8().await?;
            line.push(byte);
            if line.len() + header.len() > 8192 {
                bail!("DAP header exceeds 8 KiB");
            }
            if byte == b'\n' {
                break;
            }
        }
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        let value = std::str::from_utf8(&line)?;
        if let Some((name, value)) = value.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            if content_length.is_some() {
                bail!("Duplicate DAP Content-Length");
            }
            content_length = Some(value.trim().parse::<usize>()?);
        }
        header.extend(line);
    }
    let length = content_length.ok_or_else(|| anyhow!("Missing DAP Content-Length"))?;
    if length == 0 || length > MAX_FRAME {
        bail!("Invalid DAP frame length: {length}");
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes).await?;
    Ok(serde_json::from_slice(&bytes)?)
}

enum Outgoing {
    Request {
        id: u64,
        command: String,
        arguments: Value,
        response: oneshot::Sender<Result<Value>>,
    },
    Cancel(u64),
}

#[derive(Clone)]
pub(super) struct Client {
    outgoing: mpsc::Sender<Outgoing>,
    sequence: Arc<AtomicU64>,
    events: broadcast::Sender<Value>,
}

impl Client {
    pub async fn spawn(command: &str, args: &[String], cwd: &str) -> Result<Self> {
        let mut process = Command::new(command);
        process
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = process
            .spawn()
            .with_context(|| format!("Could not start debug adapter {command}"))?;
        let mut stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (outgoing, mut commands) = mpsc::channel(128);
        let (frames_tx, mut frames) = mpsc::channel(128);
        let (events, _) = broadcast::channel(1024);
        let sequence = Arc::new(AtomicU64::new(1));
        let client = Self {
            outgoing,
            sequence: sequence.clone(),
            events: events.clone(),
        };
        let reader = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let frame = read_frame(&mut reader).await;
                let done = frame.is_err();
                if frames_tx.send(frame).await.is_err() || done {
                    break;
                }
            }
        });
        let errors = events.clone();
        let stderr = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut chunk = [0; 4096];
            while let Ok(n) = reader.read(&mut chunk).await {
                if n == 0 {
                    break;
                }
                let _ = errors.send(json!({"type":"event", "event":"output", "body":{"category":"stderr", "output":String::from_utf8_lossy(&chunk[..n])}}));
            }
        });
        tokio::spawn(async move {
            let mut pending = HashMap::<u64, oneshot::Sender<Result<Value>>>::new();
            let result: Result<()> = async {
                loop {
                    let message = tokio::select! {
                        command = commands.recv() => match command {
                            Some(Outgoing::Request { id, command, arguments, response }) => {
                                pending.retain(|_, response| !response.is_closed());
                                if pending.len() >= 1024 {
                                    let _ = response.send(Err(anyhow!("Too many pending debug requests")));
                                    continue;
                                }
                                pending.insert(id, response);
                                Some(json!({"seq": id, "type":"request", "command":command, "arguments":arguments}))
                            }
                            Some(Outgoing::Cancel(id)) => { pending.remove(&id); None }
                            None => break,
                        },
                        frame = frames.recv() => match frame {
                            Some(Ok(frame)) => {
                                match frame["type"].as_str() {
                                    Some("response") => if let Some(id) = frame["request_seq"].as_u64() && let Some(response) = pending.remove(&id) {
                                        let result = if frame["success"].as_bool() == Some(true) { Ok(frame["body"].clone()) } else { Err(anyhow!("{}", frame["message"].as_str().unwrap_or("Debug adapter request failed"))) };
                                        let _ = response.send(result);
                                    },
                                    Some("event") => { let _ = events.send(frame); }
                                    Some("request") => {
                                        // Never launch terminal commands supplied by an adapter.
                                        let reply = json!({"seq":sequence.fetch_add(1, Ordering::Relaxed),"type":"response","request_seq":frame["seq"],"command":frame["command"],"success":false,"message":"Reverse requests are not supported by this client"});
                                        write_frame(&mut stdin, &reply).await?;
                                    }
                                    _ => {}
                                }
                                None
                            }
                            Some(Err(error)) => return Err(error),
                            None => break,
                        },
                        status = child.wait() => { let status = status?; anyhow::ensure!(status.success(), "Debug adapter exited with {status}"); break; }
                    };
                    if let Some(message) = message {
                        write_frame(&mut stdin, &message).await?;
                    }
                }
                Ok(())
            }.await;
            reader.abort();
            stderr.abort();
            let _ = child.kill().await;
            let message = result
                .err()
                .map_or("Debug adapter disconnected".into(), |e| e.to_string());
            for response in pending.into_values() {
                let _ = response.send(Err(anyhow!(message.clone())));
            }
            let _ = events.send(
                json!({"type":"event","event":"vitreDisconnected","body":{"message":message}}),
            );
        });
        Ok(client)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    pub async fn request(&self, command: &str, arguments: Value) -> Result<Value> {
        let id = self.sequence.fetch_add(1, Ordering::Relaxed);
        let (response, rx) = oneshot::channel();
        self.outgoing
            .try_send(Outgoing::Request {
                id,
                command: command.into(),
                arguments,
                response,
            })
            .map_err(|error| anyhow!("Debug request could not be queued: {error}"))?;
        match tokio::time::timeout(Duration::from_secs(20), rx).await {
            Ok(result) => result.map_err(|_| anyhow!("Debug adapter disconnected"))?,
            Err(_) => {
                let _ = self.outgoing.try_send(Outgoing::Cancel(id));
                bail!("Debug adapter timed out: {command}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn frames_are_byte_counted_and_bounded() {
        let body = serde_json::to_vec(&json!({"event":"output","body":{"output":"😀"}})).unwrap();
        let bytes = [
            format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes(),
            body,
        ]
        .concat();
        let frame = read_frame(&mut &bytes[..]).await.unwrap();
        assert_eq!(frame["body"]["output"], "😀");
        assert!(
            read_frame(&mut &b"Content-Length: 999999999\r\n\r\n"[..])
                .await
                .is_err()
        );
        assert!(
            read_frame(&mut &b"Content-Length: 2\r\nContent-Length: 2\r\n\r\n{}"[..])
                .await
                .is_err()
        );
        assert!(
            read_frame(&mut &b"Content-Length: 12\r\n\r\n{}"[..])
                .await
                .is_err()
        );
    }
}
