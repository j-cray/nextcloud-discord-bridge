use anyhow::{Result, Context};
use serenity::async_trait;
use songbird::{
    Call,
    Songbird,
    events::{Event, EventContext, EventHandler as VoiceEventHandler},
    model::payload::Speaking,
};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::media::Sample;
use std::time::Duration;
use bytes::Bytes;

use crate::nextcloud::webrtc::NextcloudWebRTC;
use crate::nextcloud::signaling::{SignalingClient, SignalingMessage};
use serenity::model::id::{GuildId, ChannelId};

pub struct DiscordToNextcloudHandler {
    pub track: Arc<TrackLocalStaticSample>,
}

#[async_trait]
impl VoiceEventHandler for DiscordToNextcloudHandler {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::RtpPacket(packet) = ctx {
            // Forward audio packet
            // packet.payload should contain the Opus frame (if decrypted)
            // Note: We are assuming Songbird decrypts it before firing RtpPacket event ??
            // Search results said "RtpPacket... does not perform audio decoding".
            // It didn't explicitly say if it decrypts.
            // But usually raw UDP is encrypted.
            // If Songbird exposes this, it might be the decrypted payload.
            // If not, we are sending garbage.
            // However, VoicePacket (deprecated) was "decrypted".
            // RtpPacket is the replacement for "raw RTP access".

             // data.packet is the raw RTP packet
             // data.payload_offset is start of payload
             // data.payload_end_pad is padding at end

             let payload = &packet.packet[packet.payload_offset..packet.packet.len() - packet.payload_end_pad];
             // println!("Got RTP packet, payload len: {}", payload.len());

            // For now, let's try to forward it.
             let sample = Sample {
                data: Bytes::copy_from_slice(payload),
                duration: Duration::from_millis(20), // standard opus frame
                ..Default::default()
            };

            if let Err(e) = self.track.write_sample(&sample).await {
                 // println!("Failed to write sample: {:?}", e);
            }
        }

        None
    }
}

pub struct BridgeSession {
    pub nextcloud: Arc<Mutex<NextcloudWebRTC>>,
    pub signaling: Arc<Mutex<SignalingClient>>,
    pub manager: Arc<Songbird>,
    pub guild_id: GuildId,
    pub channel_id: ChannelId,
}

impl BridgeSession {
    pub fn new(
        nextcloud: NextcloudWebRTC,
        signaling: SignalingClient,
        manager: Arc<Songbird>,
        guild_id: GuildId,
        channel_id: ChannelId
    ) -> Self {
        Self {
            nextcloud: Arc::new(Mutex::new(nextcloud)),
            signaling: Arc::new(Mutex::new(signaling)),
            manager,
            guild_id,
            channel_id,
        }
    }

    pub async fn start(&self) -> Result<()> {
        // 1. Join Discord
        let handler_lock = self.manager.join(self.guild_id, self.channel_id).await;
        let handler_lock = match handler_lock {
            Ok(h) => h,
            Err(e) => anyhow::bail!("Failed to join Discord channel: {:?}", e),
        };

        let mut handler = handler_lock.lock().await;

        // 2. Setup Audio Forwarding (Discord -> Nextcloud)
        {
            let nc = self.nextcloud.lock().await;
            let track = nc.audio_track.clone();

            handler.add_global_event(
                songbird::events::CoreEvent::RtpPacket.into(),
                DiscordToNextcloudHandler { track }
            );
        }
        println!("Joined Discord Channel and attached Voice Handler!");
        drop(handler); // Release lock

        // 3. Setup ICE Handling
        let (ice_tx, mut ice_rx) = mpsc::channel::<(String, String, u16)>(32);

        {
            let nc = self.nextcloud.lock().await;
            nc.on_ice_candidate(Box::new(move |candidate, mid, line| {
                 let _ = ice_tx.try_send((candidate, mid, line));
            }));
        }

        // 4. Main Event Loop
        println!("Starting Bridge Event Loop...");
        loop {
            tokio::select! {
                // Receive Local ICE candidate -> Send to Signaling
                Some((candidate, mid, line)) = ice_rx.recv() => {
                    // println!("Sending ICE candidate");
                    let mut sig = self.signaling.lock().await;
                    // Assuming we send to "server" or broadcast?
                    // For HPB, recipient might be needed or handled by server.
                    // Usually for HPB: we just send it.
                    if let Err(e) = sig.send_candidate(candidate, mid, line, "".to_string()).await {
                        println!("Error sending candidate: {:?}", e);
                    }
                }

                // Receive Signaling Message
                msg_result = async {
                    let mut sig = self.signaling.lock().await;
                    sig.next_message().await
                } => {
                     match msg_result {
                        Ok(Some(msg)) => {
                            self.handle_signaling_message(msg).await?;
                        }
                        Ok(None) => {
                            println!("Signaling connection closed");
                            break;
                        }
                        Err(e) => {
                            println!("Signaling error: {:?}", e);
                            break;
                        }
                     }
                }

                // Keep-alive/Other check?
                // _ = tokio::time::sleep(Duration::from_secs(60)) => {
                //    println!("Bridge active...");
                // }
            }
        }

        Ok(())
    }

    async fn handle_signaling_message(&self, msg: SignalingMessage) -> Result<()> {
        match msg {
            SignalingMessage::Hello { .. } => {},
            SignalingMessage::Joined { .. } => {
                println!("Joined Nextcloud Room successfully!");
            },
            SignalingMessage::Message { data } => {
                // Handle Offer/Answer/Candidate
                // data is JSON Value
                let type_ = data.get("type").and_then(|v| v.as_str());
                match type_ {
                    Some("offer") => {
                         println!("Received Offer");
                         if let Some(sdp) = data.get("sdp").and_then(|v| v.as_str()) {
                             let nc = self.nextcloud.lock().await;
                             let answer_sdp = nc.handle_offer(sdp.to_string()).await?;

                             let mut sig = self.signaling.lock().await;
                             // Send Answer
                             // Recipient? usually whoever sent the offer.
                             // But in HPB/Janus, we usually reply to the backend.
                             let sender = data.get("sender").and_then(|v| v.as_str()).unwrap_or("");
                             sig.send_sdp("answer", answer_sdp, sender.to_string()).await?;
                             println!("Sent Answer");
                         }
                    },
                    Some("answer") => {
                         println!("Received Answer");
                         if let Some(sdp) = data.get("sdp").and_then(|v| v.as_str()) {
                             let nc = self.nextcloud.lock().await;
                             nc.handle_answer(sdp.to_string()).await?;
                             println!("Handled Answer");
                         }
                    },
                    Some("candidate") => {
                         // println!("Received Candidate");
                         if let (Some(cand), Some(mid), Some(line)) = (
                             data.get("candidate").and_then(|v| v.as_str()),
                             data.get("sdpMid").and_then(|v| v.as_str()),
                             data.get("sdpMLineIndex").and_then(|v| v.as_u64())
                         ) {
                             let nc = self.nextcloud.lock().await;
                             nc.add_ice_candidate(cand.to_string(), mid.to_string(), line as u16).await?;
                         }
                    },
                    _ => {}
                }
            },
            _ => {}
        }
        Ok(())
    }
}
