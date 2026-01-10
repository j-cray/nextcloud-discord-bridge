use anyhow::{Result, Context};
use std::sync::Arc;
use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;

pub struct NextcloudWebRTC {
    pub peer_connection: Arc<RTCPeerConnection>,
    pub audio_track: Arc<TrackLocalStaticSample>,
}

impl NextcloudWebRTC {
    pub async fn new() -> Result<Self> {
        // Create a MediaEngine object to configure the supported codec
        let mut m = MediaEngine::default();
        m.register_default_codecs()?;

        // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
        // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
        // this is enabled by default. If you use `APIBuilder` you must enable it yourself.
        let mut registry = Registry::new();

        // Use the default set of Interceptors
        registry = register_default_interceptors(registry, &mut m)?;

        // Create the API object with the MediaEngine
        let api = APIBuilder::new()
            .with_media_engine(m)
            .with_interceptor_registry(registry)
            .build();

        // Prepare the configuration
        // TODO: Get TURN servers from Nextcloud Signaling!
        let config = RTCConfiguration {
            ice_servers: vec![RTCIceServer {
                urls: vec!["stun:stun.l.google.com:19302".to_owned()],
                ..Default::default()
            }],
            ..Default::default()
        };

        // Create a new RTCPeerConnection
        let peer_connection = api.new_peer_connection(config).await?;

        // Create a local audio track (Opus)
        let audio_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: "audio/opus".to_owned(),
                ..Default::default()
            },
            "audio".to_owned(),
            "webrtc-rs".to_owned(),
        ));

        // Add this track to the PeerConnection
        peer_connection
            .add_track(Arc::clone(&audio_track) as Arc<dyn TrackLocal + Send + Sync>)
            .await?;

        // Set the handler for Peer connection state
        // This will notify you when the peer has connected/disconnected
         peer_connection
            .on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
                println!("Peer Connection State has changed: {s}");
                Box::pin(async {})
            }));

        Ok(Self {
            peer_connection: Arc::new(peer_connection),
            audio_track,
        })
    }

    // Register callback for local ICE candidates
    pub fn on_ice_candidate(&self, f: Box<dyn Fn(String, String, u16) + Send + Sync>) {
        let f = Arc::new(f);
        self.peer_connection.on_ice_candidate(Box::new(move |c| {
            let f = f.clone();
            Box::pin(async move {
                if let Some(c) = c {
                    if let Ok(json) = c.to_json() {
                         let sdp = json.candidate;
                         let mid = json.sdp_mid.unwrap_or_default();
                         let line = json.sdp_mline_index.unwrap_or(0);
                         f(sdp, mid, line);
                    }
                }
            })
        }));
    }

    pub async fn handle_offer(&self, sdp: String) -> Result<String> {
        let desc = RTCSessionDescription::offer(sdp)?;
        self.peer_connection.set_remote_description(desc).await?;

        let answer = self.peer_connection.create_answer(None).await?;
        let answer_sdp = answer.sdp.clone();

        // Poller starts gathering ICE candidates here usually
        self.peer_connection.set_local_description(answer).await?;

        Ok(answer_sdp)
    }

    pub async fn handle_answer(&self, sdp: String) -> Result<()> {
        let desc = RTCSessionDescription::answer(sdp)?;
        self.peer_connection.set_remote_description(desc).await?;
        Ok(())
    }

    pub async fn add_ice_candidate(&self, candidate: String, sdp_mid: String, sdp_mline_index: u16) -> Result<()> {
        let candidate_init = RTCIceCandidateInit {
            candidate,
            sdp_mid: Some(sdp_mid),
            sdp_mline_index: Some(sdp_mline_index),
            username_fragment: None,
        };

        self.peer_connection.add_ice_candidate(candidate_init).await?;
        Ok(())
    }
}
