use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::tungstenite::protocol::Message;
use url::Url;

#[derive(Debug, Clone)]
pub struct Config {
    pub nextcloud_url: String,
    pub username: String,
    pub password: String, // Or token
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SignalingMessage {
    Hello {
        version: String,
        checksum: String,
    },
    Join {
        #[serde(rename = "roomType")]
        room_type: String,
        #[serde(rename = "roomToken")]
        room_token: String,
        #[serde(rename = "participantToken")]
        participant_token: String,
    },
    Joined {
        #[serde(rename = "roomType")]
        room_type: String,
        quit: bool,
    },
    Message {
        data: Value,
    },
    Bye,
}


pub struct SignalingClient {
    config: Config,
    socket: Option<WebSocketStream<MaybeTlsStream<TcpStream>>>,
}

impl SignalingClient {
    pub fn new(config: Config) -> Self {
        Self { config, socket: None }
    }

    pub async fn connect(&mut self, room_token: &str) -> Result<()> {
        let base_url = Url::parse(&self.config.nextcloud_url)
            .context("Invalid Nextcloud URL")?;

        // 1. Call Nextcloud API to get Signaling credentials
        // Endpoint: /ocs/v2.php/apps/spreed/api/v4/room/{token}
        // Note: For public rooms, authentication might be optional or handled differently.
        // For now assuming logged-in user or at least valid credentials provided in Config.
        let api_url = base_url.join(&format!("/ocs/v2.php/apps/spreed/api/v4/room/{}", room_token))?;

        println!("Fetching room details from: {}", api_url);

        let client = reqwest::Client::new();
        let resp = client.get(api_url.clone())
            .basic_auth(&self.config.username, Some(&self.config.password))
            .header("OCS-APIRequest", "true")
            .header("Accept", "application/json")
            .send()
            .await
            .context("Failed to send request to Nextcloud")?;

        if !resp.status().is_success() {
            anyhow::bail!("Nextcloud API returned error: {}", resp.status());
        }

        let body: serde_json::Value = resp.json().await.context("Failed to parse Nextcloud response")?;

        // 2. Extract Signaling settings
        // Expected structure: ocs.data.signaling.url and ocs.data.signaling.ticket
        let signaling = body.get("ocs")
            .and_then(|o| o.get("data"))
            .and_then(|d| d.get("signaling"))
            .context("No signaling info found in response")?;

        // Handling both internal signaling (no dedicated URL usually, just standard repeated poll)
        // OR High Performance Backend (HPB) which gives a URL.
        // This implementation focuses on HPB (WebSocket).

        let ws_url_str = signaling.get("url").and_then(|v| v.as_str())
            .context("No signaling URL found (is High Performance Backend enabled?)")?;

        let ticket = signaling.get("ticket").and_then(|v| v.as_str())
             .context("No signaling ticket found")?;

        println!("Connecting to Signaling Server: {}", ws_url_str);

        let (ws_stream, _) = connect_async(ws_url_str).await
            .context("Failed to connect to Signaling WebSocket")?;

        println!("WebSocket connected!");
        self.socket = Some(ws_stream);

        // 3. Handshake
        // First message should be Hello from server? Or we wait for it.
        // Usually: Client connects.
        // Server sends: {"type":"hello", ...}
        // Client sends: {"type":"hello", ...} (optional/dependent on version)
        // Client sends: {"type":"join", ...}

        // Let's implement a simple loop outside or helper here to authenticate
        self.authenticate(room_token, ticket).await?;

        Ok(())
    }

    async fn authenticate(&mut self, room_token: &str, ticket: &str) -> Result<()> {
        let socket = self.socket.as_mut().context("Not connected")?;

        // Read Hello
        if let Some(msg) = socket.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                println!("Received: {}", text);
                 // TODO: Validate Hello
            }
        }

        // Send Join
        let join_msg = serde_json::json!({
            "type": "join",
            "roomType": "room", // token is for a room
            "roomToken": room_token,
             "participantToken": ticket,
        });

        socket.send(Message::Text(join_msg.to_string())).await?;
        println!("Sent Join request");

        // Wait for Joined
         if let Some(msg) = socket.next().await {
            let msg = msg?;
            if let Message::Text(text) = msg {
                 println!("Received after join: {}", text);
                 // Expect "joined"
            }
        }

        Ok(())
    }

    pub async fn next_message(&mut self) -> Result<Option<SignalingMessage>> {
        let socket = self.socket.as_mut().context("Not connected")?;

        while let Some(msg) = socket.next().await {
            let msg = msg?;
            match msg {
                Message::Text(text) => {
                    // println!("Raw Message: {}", text); // Debug
                    match serde_json::from_str::<SignalingMessage>(&text) {
                        Ok(parsed) => return Ok(Some(parsed)),
                        Err(e) => {
                             println!("Failed to parse message: {}. Error: {}", text, e);
                             continue;
                        }
                    }
                }
                Message::Close(_) => return Ok(None),
                _ => continue,
            }
        }
        Ok(None)
    }

    pub async fn send_sdp(&mut self, sdp_type: &str, sdp: String, recipient: String) -> Result<()> {
        let socket = self.socket.as_mut().context("Not connected")?;

        // Structure for sending messages in Nextcloud Talk Signaling
        // { "type": "message", "data": { "type": "offer", "sdp": "...", "roomToken": "..." } }
        // Note: The recipient handling might depend on if it's p2p or mcu.
        // For HPB (MCU), we usually send to the server.

        let payload = serde_json::json!({
            "type": "message",
            "data": {
                "type": sdp_type, // "offer" or "answer"
                "sdp": sdp,
                "recipient": recipient
            }
        });

        socket.send(Message::Text(payload.to_string())).await?;
        Ok(())
    }

    pub async fn send_candidate(&mut self, candidate: String, sdp_mid: String, sdp_mline_index: u16, recipient: String) -> Result<()> {
         let socket = self.socket.as_mut().context("Not connected")?;

         let payload = serde_json::json!({
            "type": "message",
            "data": {
                "type": "candidate",
                "candidate": candidate,
                "sdpMid": sdp_mid,
                "sdpMLineIndex": sdp_mline_index,
                 "recipient": recipient
            }
        });

        socket.send(Message::Text(payload.to_string())).await?;
        Ok(())
    }
}
