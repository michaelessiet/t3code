//! HTTP-side auth against the sidecar: the form-urlencoded `/oauth/token`
//! bootstrap exchange (`AuthTokenExchangeRequest` in
//! `packages/contracts/src/auth.ts`) and the `/api/auth/websocket-ticket`
//! ticket that gates `/ws`.

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;

use crate::error::RpcError;

const TOKEN_EXCHANGE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";
const ACCESS_TOKEN_TYPE: &str = "urn:ietf:params:oauth:token-type:access_token";
const ENVIRONMENT_BOOTSTRAP_TOKEN_TYPE: &str =
    "urn:t3:params:oauth:token-type:environment-bootstrap";

#[derive(Debug, Clone, Deserialize)]
pub struct AccessToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: f64,
    pub scope: String,
}

#[derive(Debug, Deserialize)]
struct WebSocketTicket {
    ticket: String,
}

pub struct EnvironmentHttp {
    /// e.g. `http://127.0.0.1:3773` — always 127.0.0.1, never localhost
    /// (the server binds IPv4 only; `localhost` can resolve IPv6-first).
    pub base_url: String,
    client: reqwest::Client,
}

impl EnvironmentHttp {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
        }
    }

    pub(crate) fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Exchange the sidecar bootstrap token (delivered to the server via the
    /// stdin envelope) for a bearer access token.
    pub async fn exchange_bootstrap_token(
        &self,
        bootstrap_token: &str,
    ) -> Result<AccessToken, RpcError> {
        let response = self
            .client
            .post(format!("{}/oauth/token", self.base_url))
            .form(&[
                ("grant_type", TOKEN_EXCHANGE_GRANT_TYPE),
                ("subject_token", bootstrap_token),
                ("subject_token_type", ENVIRONMENT_BOOTSTRAP_TOKEN_TYPE),
                ("requested_token_type", ACCESS_TOKEN_TYPE),
                ("client_label", "Vitre"),
                ("client_device_type", "desktop"),
            ])
            .send()
            .await?;
        Ok(check(response).await?.json().await?)
    }

    /// Issue a short-lived one-time ticket for the `/ws` handshake.
    pub async fn websocket_ticket(&self, access_token: &str) -> Result<String, RpcError> {
        let response = self
            .client
            .post(format!("{}/api/auth/websocket-ticket", self.base_url))
            .bearer_auth(access_token)
            .send()
            .await?;
        let ticket: WebSocketTicket = check(response).await?.json().await?;
        Ok(ticket.ticket)
    }

    /// `ws://…/ws?wsTicket=<ticket>` for [`crate::RpcSession::connect`].
    pub fn ws_url(&self, ticket: &str) -> String {
        let ws_base = self
            .base_url
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1);
        format!(
            "{ws_base}/ws?wsTicket={}",
            utf8_percent_encode(ticket, NON_ALPHANUMERIC)
        )
    }
}

pub(crate) async fn check(response: reqwest::Response) -> Result<reqwest::Response, RpcError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(RpcError::HttpStatus {
        status: status.as_u16(),
        body,
    })
}
