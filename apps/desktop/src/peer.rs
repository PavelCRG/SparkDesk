use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use tokio::sync::Notify;
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCIceServer, RTCPeerConnectionState, RTCSessionDescription,
};

#[derive(Clone)]
struct Handler {
    gathered: Arc<Notify>,
    connected: Arc<AtomicBool>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.notify_one();
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        self.connected.store(
            state == RTCPeerConnectionState::Connected,
            Ordering::Release,
        );
    }
}

pub struct PeerSession {
    connection: Arc<dyn PeerConnection>,
    connected: Arc<AtomicBool>,
}

impl PeerSession {
    pub async fn offer() -> Result<(Self, String), String> {
        let (session, gathered) = Self::new().await?;
        session
            .connection
            .create_data_channel("control", None)
            .await
            .map_err(|error| error.to_string())?;
        session
            .connection
            .create_data_channel("screen", None)
            .await
            .map_err(|error| error.to_string())?;
        let offer = session
            .connection
            .create_offer(None)
            .await
            .map_err(|error| error.to_string())?;
        session
            .connection
            .set_local_description(offer)
            .await
            .map_err(|error| error.to_string())?;
        wait_for_gathering(gathered).await?;
        let description = session
            .connection
            .local_description()
            .await
            .ok_or_else(|| "no local offer was produced".to_owned())?;
        let sdp = serde_json::to_string(&description).map_err(|error| error.to_string())?;
        Ok((session, sdp))
    }

    pub async fn answer(offer_sdp: &str) -> Result<(Self, String), String> {
        let (session, gathered) = Self::new().await?;
        let offer: RTCSessionDescription =
            serde_json::from_str(offer_sdp).map_err(|error| format!("invalid offer: {error}"))?;
        session
            .connection
            .set_remote_description(offer)
            .await
            .map_err(|error| error.to_string())?;
        let answer = session
            .connection
            .create_answer(None)
            .await
            .map_err(|error| error.to_string())?;
        session
            .connection
            .set_local_description(answer)
            .await
            .map_err(|error| error.to_string())?;
        wait_for_gathering(gathered).await?;
        let description = session
            .connection
            .local_description()
            .await
            .ok_or_else(|| "no local answer was produced".to_owned())?;
        let sdp = serde_json::to_string(&description).map_err(|error| error.to_string())?;
        Ok((session, sdp))
    }

    pub async fn set_answer(&self, answer_sdp: &str) -> Result<(), String> {
        let answer: RTCSessionDescription =
            serde_json::from_str(answer_sdp).map_err(|error| format!("invalid answer: {error}"))?;
        self.connection
            .set_remote_description(answer)
            .await
            .map_err(|error| error.to_string())
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire)
    }

    pub async fn close(&self) {
        let _ = self.connection.close().await;
    }

    async fn new() -> Result<(Self, Arc<Notify>), String> {
        let gathered = Arc::new(Notify::new());
        let connected = Arc::new(AtomicBool::new(false));
        let configuration = RTCConfigurationBuilder::new()
            .with_ice_servers(vec![RTCIceServer {
                urls: vec!["stun:stun.cloudflare.com:3478".to_owned()],
                ..Default::default()
            }])
            .build();
        let connection = PeerConnectionBuilder::new()
            .with_configuration(configuration)
            .with_handler(Arc::new(Handler {
                gathered: gathered.clone(),
                connected: connected.clone(),
            }))
            .with_udp_addrs(vec!["0.0.0.0:0"])
            .build()
            .await
            .map_err(|error| error.to_string())?;
        Ok((
            Self {
                connection: Arc::new(connection),
                connected,
            },
            gathered,
        ))
    }
}

async fn wait_for_gathering(gathered: Arc<Notify>) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(20), gathered.notified())
        .await
        .map_err(|_| "ICE gathering timed out; check firewall and STUN access".to_owned())
        .map(|_| ())
}
