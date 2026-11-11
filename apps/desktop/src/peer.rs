use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use bytes::BytesMut;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, Notify, mpsc};
use webrtc::data_channel::{
    DataChannel, DataChannelEvent, RTCDataChannelInit, RTCDataChannelState,
};
use webrtc::peer_connection::{
    PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCConfigurationBuilder,
    RTCIceGatheringState, RTCIceServer, RTCPeerConnectionState, RTCSessionDescription,
};

use crate::{
    diagnostics::{PeerConnectionStatus, SessionDiagnostics},
    identity::{DeviceIdentity, verify_peer_proof},
    input::{HeldInput, InputRateLimit, MonitorGeometry, RemoteInput, validate_input},
    screen::{
        FrameAssembler, SCREEN_FRAME_INTERVAL, SCREEN_SEND_BUFFER_BYTES, can_capture_screen_frame,
        capture_primary, chunk_frame_iter, validate_jpeg,
    },
};

const DATA_CHANNEL_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_SDP_BYTES: usize = 256 * 1024;

type ChannelMap = Arc<Mutex<HashMap<String, Arc<dyn DataChannel>>>>;

struct PeerEvent {
    label: String,
    event: DataChannelEvent,
}

#[derive(Clone)]
struct Handler {
    gathered: Arc<Notify>,
    connected: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    diagnostics: Arc<SessionDiagnostics>,
    incoming_channels: mpsc::Sender<Arc<dyn DataChannel>>,
}

#[async_trait::async_trait]
impl PeerConnectionEventHandler for Handler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        if state == RTCIceGatheringState::Complete {
            self.gathered.notify_one();
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        let status = match state {
            RTCPeerConnectionState::Connected => PeerConnectionStatus::Connected,
            RTCPeerConnectionState::Disconnected => PeerConnectionStatus::Disconnected,
            RTCPeerConnectionState::Failed => PeerConnectionStatus::Failed,
            RTCPeerConnectionState::Closed => PeerConnectionStatus::Closed,
            RTCPeerConnectionState::New | RTCPeerConnectionState::Connecting => {
                PeerConnectionStatus::Connecting
            }
            _ => PeerConnectionStatus::Connecting,
        };
        self.diagnostics.set_connection_status(status);
        self.connected.store(
            state == RTCPeerConnectionState::Connected,
            Ordering::Release,
        );
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            self.failed.store(true, Ordering::Release);
        }
    }

    async fn on_data_channel(&self, channel: Arc<dyn DataChannel>) {
        let _ = self.incoming_channels.send(channel).await;
    }
}

pub struct PeerSession {
    connection: Arc<dyn PeerConnection>,
    connected: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
    authenticated: Arc<AtomicBool>,
    channels: ChannelMap,
    events: Option<mpsc::Receiver<PeerEvent>>,
    event_tx: mpsc::Sender<PeerEvent>,
    offer_sdp: Option<String>,
    answer_sdp: Option<String>,
    running: Arc<AtomicBool>,
    diagnostics: Arc<SessionDiagnostics>,
}

impl PeerSession {
    pub async fn offer() -> Result<(Self, String), String> {
        let (mut session, gathered) = Self::new().await?;
        let control = session
            .connection
            .create_data_channel("control", None)
            .await
            .map_err(|error| error.to_string())?;
        let screen = session
            .connection
            .create_data_channel("screen", Some(screen_data_channel_init()))
            .await
            .map_err(|error| error.to_string())?;
        register_channel(control, session.channels.clone(), session.event_sender()).await;
        register_channel(screen, session.channels.clone(), session.event_sender()).await;
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
        if sdp.len() > MAX_SDP_BYTES {
            return Err("local offer exceeds the SDP size limit".into());
        }
        session.offer_sdp = Some(sdp.clone());
        Ok((session, sdp))
    }

    pub async fn answer(offer_sdp: &str) -> Result<(Self, String), String> {
        if offer_sdp.len() > MAX_SDP_BYTES {
            return Err("remote offer exceeds the SDP size limit".into());
        }
        let (mut session, gathered) = Self::new().await?;
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
        if sdp.len() > MAX_SDP_BYTES {
            return Err("local answer exceeds the SDP size limit".into());
        }
        session.offer_sdp = Some(offer_sdp.to_owned());
        session.answer_sdp = Some(sdp.clone());
        Ok((session, sdp))
    }

    pub async fn set_answer(&mut self, answer_sdp: &str) -> Result<(), String> {
        if answer_sdp.len() > MAX_SDP_BYTES {
            return Err("remote answer exceeds the SDP size limit".into());
        }
        let answer: RTCSessionDescription =
            serde_json::from_str(answer_sdp).map_err(|error| format!("invalid answer: {error}"))?;
        self.connection
            .set_remote_description(answer)
            .await
            .map_err(|error| error.to_string())?;
        self.answer_sdp = Some(answer_sdp.to_owned());
        Ok(())
    }

    pub async fn authenticate(
        &mut self,
        identity: &DeviceIdentity,
        local_id: &str,
        remote_id: &str,
        initiator_id: &str,
        target_id: &str,
        request_id: &str,
    ) -> Result<(), String> {
        let control = self.wait_channel("control").await?;
        let _screen = self.wait_channel("screen").await?;
        tokio::time::timeout(DATA_CHANNEL_TIMEOUT, async {
            loop {
                if control.ready_state().await.ok() == Some(RTCDataChannelState::Open) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| "WebRTC control channel did not open".to_owned())?;

        let mut local_nonce = [0u8; 32];
        getrandom::fill(&mut local_nonce).map_err(|error| error.to_string())?;
        let local_nonce = sparkdesk_protocol::encode_hex(&local_nonce);
        let hello = HandshakeMessage::Hello {
            device_id: local_id.to_owned(),
            public_key: identity.public_key_hex(),
            request_id: request_id.to_owned(),
            nonce: local_nonce.clone(),
        };
        control
            .send_text(&serde_json::to_string(&hello).map_err(|error| error.to_string())?)
            .await
            .map_err(|error| error.to_string())?;

        let (remote_public_key, remote_nonce) = tokio::time::timeout(DATA_CHANNEL_TIMEOUT, async {
            loop {
                let event = self
                    .events
                    .as_mut()
                    .ok_or("peer event stream unavailable")?
                    .recv()
                    .await
                    .ok_or("peer data channel closed")?;
                if event.label != "control" {
                    continue;
                }
                let DataChannelEvent::OnMessage(message) = event.event else {
                    continue;
                };
                if !message.is_string || message.data.len() > 1024 {
                    return Err("invalid peer authentication message");
                }
                let HandshakeMessage::Hello {
                    device_id,
                    public_key,
                    request_id: peer_request_id,
                    nonce,
                } = serde_json::from_slice(&message.data)
                    .map_err(|_| "invalid peer authentication hello")?
                else {
                    continue;
                };
                let public_key =
                    decode_fixed::<32>(&public_key).ok_or("invalid peer public key")?;
                if device_id != remote_id
                    || peer_request_id != request_id
                    || sparkdesk_protocol::device_id_from_public_key(&public_key) != remote_id
                    || decode_fixed::<32>(&nonce).is_none()
                {
                    return Err("peer identity does not match this request");
                }
                return Ok((public_key, nonce));
            }
        })
        .await
        .map_err(|_| "peer authentication timed out".to_owned())??;

        let (remote_public_key, remote_nonce) = (remote_public_key, remote_nonce);
        let (initiator_nonce, target_nonce) = if local_id == initiator_id {
            (&local_nonce, &remote_nonce)
        } else {
            (&remote_nonce, &local_nonce)
        };
        let context = peer_context(
            request_id,
            initiator_id,
            target_id,
            initiator_nonce,
            target_nonce,
            self.offer_sdp.as_deref().ok_or("offer not stored")?,
            self.answer_sdp.as_deref().ok_or("answer not stored")?,
        );
        let proof = HandshakeMessage::Proof {
            device_id: local_id.to_owned(),
            signature: sparkdesk_protocol::encode_hex(&identity.sign_peer_proof(&context)),
        };
        control
            .send_text(&serde_json::to_string(&proof).map_err(|error| error.to_string())?)
            .await
            .map_err(|error| error.to_string())?;

        tokio::time::timeout(DATA_CHANNEL_TIMEOUT, async {
            loop {
                let event = self
                    .events
                    .as_mut()
                    .ok_or("peer event stream unavailable")?
                    .recv()
                    .await
                    .ok_or("peer data channel closed")?;
                if event.label != "control" {
                    continue;
                }
                let DataChannelEvent::OnMessage(message) = event.event else {
                    continue;
                };
                if !message.is_string || message.data.len() > 1024 {
                    return Err("invalid peer authentication proof");
                }
                let HandshakeMessage::Proof {
                    device_id,
                    signature,
                } = serde_json::from_slice(&message.data)
                    .map_err(|_| "invalid peer authentication proof")?
                else {
                    continue;
                };
                let signature = decode_fixed::<64>(&signature)
                    .ok_or("invalid peer authentication signature")?;
                if device_id != remote_id
                    || !verify_peer_proof(remote_id, remote_public_key, signature, &context)
                {
                    return Err("peer authentication failed");
                }
                return Ok(());
            }
        })
        .await
        .map_err(|_| "peer authentication timed out".to_owned())??;
        self.authenticated.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn wait_channel(&self, label: &str) -> Result<Arc<dyn DataChannel>, String> {
        tokio::time::timeout(DATA_CHANNEL_TIMEOUT, async {
            loop {
                if let Some(channel) = self.channels.lock().await.get(label).cloned() {
                    return Ok(channel);
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .map_err(|_| format!("WebRTC {label} channel did not arrive"))?
    }

    fn event_sender(&self) -> mpsc::Sender<PeerEvent> {
        self.event_tx.clone()
    }

    pub async fn start_data_plane(
        &mut self,
        host: bool,
        app: AppHandle,
        control_allowed: Arc<AtomicBool>,
    ) -> Result<(), String> {
        if !self.authenticated.load(Ordering::Acquire) {
            return Err("peer identity must be verified before starting media".into());
        }
        let screen = self.wait_channel("screen").await?;
        let control = self.wait_channel("control").await?;
        let events = self
            .events
            .take()
            .ok_or("peer data channels already started")?;
        let diagnostics = self.diagnostics.clone();
        tokio::spawn(run_data_events(
            events,
            control,
            host,
            app,
            control_allowed,
            MonitorGeometry::primary().ok(),
            diagnostics.clone(),
        ));
        if host {
            let running = self.running.clone();
            tokio::spawn(async move {
                send_screen_frames(screen, running, diagnostics).await;
            });
        }
        self.send_permission(false).await?;
        Ok(())
    }

    pub async fn send_permission(&self, enabled: bool) -> Result<(), String> {
        self.send_control(&PeerControlMessage::Permission { enabled })
            .await
    }

    pub async fn send_input(&self, sequence: u32, input: RemoteInput) -> Result<(), String> {
        validate_input(&input).map_err(str::to_owned)?;
        self.send_control(&PeerControlMessage::Input { sequence, input })
            .await?;
        self.diagnostics.input_sent();
        Ok(())
    }

    pub fn record_rendered_frames(&self, count: u32, last_id: u32) {
        self.diagnostics.rendered_batch(u64::from(count), last_id);
    }

    async fn send_control(&self, message: &PeerControlMessage) -> Result<(), String> {
        let control = self.wait_channel("control").await?;
        let text = serde_json::to_string(message).map_err(|error| error.to_string())?;
        control
            .send_text(&text)
            .await
            .map_err(|error| error.to_string())
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Acquire) && self.authenticated.load(Ordering::Acquire)
    }

    pub fn is_failed(&self) -> bool {
        self.failed.load(Ordering::Acquire)
    }

    pub async fn close(&self) {
        self.running.store(false, Ordering::Release);
        let _ = self.connection.close().await;
    }

    async fn new() -> Result<(Self, Arc<Notify>), String> {
        let gathered = Arc::new(Notify::new());
        let connected = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let authenticated = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        let diagnostics = Arc::new(SessionDiagnostics::default());
        let channels = Arc::new(Mutex::new(HashMap::new()));
        let (event_tx, events) = mpsc::channel(16);
        let (incoming_channels, mut incoming_rx) = mpsc::channel(4);
        let register_channels = channels.clone();
        let register_events = event_tx.clone();
        tokio::spawn(async move {
            while let Some(channel) = incoming_rx.recv().await {
                register_channel(channel, register_channels.clone(), register_events.clone()).await;
            }
        });
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
                failed: failed.clone(),
                diagnostics: diagnostics.clone(),
                // The UI must wait for the signed peer challenge before declaring a session ready.
                incoming_channels,
            }))
            .with_data_channel_send_buffer_limit(SCREEN_SEND_BUFFER_BYTES)
            .with_udp_addrs(vec!["0.0.0.0:0"])
            .build()
            .await
            .map_err(|error| error.to_string())?;
        Ok((
            Self {
                connection: Arc::new(connection),
                connected,
                failed,
                authenticated,
                channels,
                events: Some(events),
                event_tx,
                offer_sdp: None,
                answer_sdp: None,
                running,
                diagnostics,
            },
            gathered,
        ))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum HandshakeMessage {
    Hello {
        device_id: String,
        public_key: String,
        request_id: String,
        nonce: String,
    },
    Proof {
        device_id: String,
        signature: String,
    },
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum PeerControlMessage {
    Permission { enabled: bool },
    Input { sequence: u32, input: RemoteInput },
    Ack { sequence: u32, applied: bool },
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenFrameEvent {
    id: u32,
    width: u16,
    height: u16,
    jpeg_base64: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct InputAckEvent {
    sequence: u32,
    applied: bool,
}

fn screen_data_channel_init() -> RTCDataChannelInit {
    RTCDataChannelInit {
        ordered: false,
        max_retransmits: Some(0),
        ..Default::default()
    }
}

async fn send_screen_frames(
    channel: Arc<dyn DataChannel>,
    running: Arc<AtomicBool>,
    diagnostics: Arc<SessionDiagnostics>,
) {
    let mut interval = tokio::time::interval(SCREEN_FRAME_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut frame_id = 0u32;
    while running.load(Ordering::Acquire) {
        interval.tick().await;
        if !running.load(Ordering::Acquire) {
            break;
        }
        let Ok(outstanding) = channel.outstanding_bytes().await else {
            break;
        };
        if !can_capture_screen_frame(outstanding) {
            diagnostics.skipped_backlog();
            continue;
        }
        let Ok(Ok((mut frame, timings))) = tokio::task::spawn_blocking(capture_primary).await
        else {
            continue;
        };
        frame.id = frame_id;
        frame_id = frame_id.wrapping_add(1);
        let frame_bytes = frame.jpeg.len();
        let id = frame.id;
        diagnostics.capture(
            id,
            frame_bytes,
            timings.capture_us,
            timings.resize_us,
            timings.encode_us,
        );
        let Ok(chunks) = chunk_frame_iter(&frame) else {
            continue;
        };
        let send_started = Instant::now();
        let mut fully_sent = true;
        for chunk in chunks {
            if !running.load(Ordering::Acquire)
                || channel
                    .send(BytesMut::from(chunk.as_slice()))
                    .await
                    .is_err()
            {
                fully_sent = false;
                break;
            }
        }
        diagnostics.send_duration(send_started.elapsed().as_micros() as u64);
        if fully_sent {
            diagnostics.sent(id, frame_bytes);
        }
    }
}

async fn run_data_events(
    mut events: mpsc::Receiver<PeerEvent>,
    control: Arc<dyn DataChannel>,
    host: bool,
    app: AppHandle,
    control_allowed: Arc<AtomicBool>,
    monitor: Option<MonitorGeometry>,
    diagnostics: Arc<SessionDiagnostics>,
) {
    let mut frames = FrameAssembler::default();
    let mut input_limit = InputRateLimit::new(Instant::now());
    let mut held = HeldInput::default();
    let mut permission_poll = tokio::time::interval(Duration::from_millis(50));
    let mut diagnostics_poll = tokio::time::interval(Duration::from_secs(1));
    diagnostics_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            event = events.recv() => {
                let Some(event) = event else { break; };
                let message = match event.event {
                    DataChannelEvent::OnMessage(message) => message,
                    DataChannelEvent::OnClose => break,
                    _ => continue,
                };
                if event.label == "screen" && !host && !message.is_string {
                    let incomplete_before = frames.incomplete_frames_dropped();
                    let stale_before = frames.stale_chunks_dropped();
                    let frame = frames.push(&message.data);
                    if let Some(elapsed_us) = frames.take_reassembly_us() {
                        diagnostics.reassembly_duration(elapsed_us);
                    }
                    if frames.incomplete_frames_dropped() > incomplete_before {
                        diagnostics.dropped_incomplete_frame();
                    }
                    if frames.stale_chunks_dropped() > stale_before {
                        diagnostics.dropped_stale_chunk();
                    }
                    if let Ok(Some(frame)) = frame {
                        if validate_jpeg(&frame) {
                            diagnostics.received(frame.id, frame.jpeg.len());
                            let payload = ScreenFrameEvent {
                                id: frame.id,
                                width: frame.width,
                                height: frame.height,
                                jpeg_base64: BASE64.encode(frame.jpeg),
                            };
                            let event_started = Instant::now();
                            let _ = app.emit("screen-frame", payload);
                            diagnostics.event_duration(event_started.elapsed().as_micros() as u64);
                        }
                    }
                } else if event.label == "control" && message.is_string && message.data.len() <= crate::input::MAX_INPUT_BYTES {
                    if let Ok(control_message) = serde_json::from_slice::<PeerControlMessage>(&message.data) {
                        match control_message {
                            PeerControlMessage::Permission { enabled } if !host => {
                                crate::signaling::set_control_status(&app, enabled);
                            }
                            PeerControlMessage::Input { sequence, input } if host => {
                                let mut applied = control_allowed.load(Ordering::Acquire)
                                    && validate_input(&input).is_ok()
                                    && input_limit.allow(Instant::now());
                                #[cfg(target_os = "windows")]
                                {
                                    applied = applied
                                        && monitor.is_some_and(|monitor| held.apply(input, monitor).is_ok());
                                }
                                #[cfg(not(target_os = "windows"))]
                                {
                                    let _ = input;
                                    applied = false;
                                }
                                diagnostics.input_dispatched(applied);
                                if let Ok(text) = serde_json::to_string(&PeerControlMessage::Ack { sequence, applied }) {
                                    if control.send_text(&text).await.is_ok() {
                                        diagnostics.input_ack_sent();
                                    }
                                }
                            }
                            PeerControlMessage::Ack { sequence, applied } if !host => {
                                diagnostics.input_ack_received();
                                let _ = app.emit("input-ack", InputAckEvent { sequence, applied });
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ = permission_poll.tick(), if host => {
                if !control_allowed.load(Ordering::Acquire) {
                    #[cfg(target_os = "windows")]
                    held.release_all();
                }
            }
            _ = diagnostics_poll.tick() => {
                let snapshot = diagnostics.snapshot();
                let _ = app.emit("session-diagnostics", snapshot);
            }
        }
    }
    if frames.discard_incomplete_frame() {
        diagnostics.dropped_incomplete_frame();
    }
    #[cfg(target_os = "windows")]
    held.release_all();
}

impl Drop for PeerSession {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Release);
        let connection = self.connection.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = connection.close().await;
            });
        }
    }
}

fn peer_context(
    request_id: &str,
    initiator_id: &str,
    target_id: &str,
    initiator_nonce: &str,
    target_nonce: &str,
    offer_sdp: &str,
    answer_sdp: &str,
) -> Vec<u8> {
    format!(
        "SparkDesk peer session v1\0{request_id}\0{initiator_id}\0{target_id}\0{initiator_nonce}\0{target_nonce}\0{offer_sdp}\0{answer_sdp}"
    )
    .into_bytes()
}

fn decode_fixed<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = [0; N];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        output[index] = (hex_value(pair[0])? << 4) | hex_value(pair[1])?;
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

async fn register_channel(
    channel: Arc<dyn DataChannel>,
    channels: ChannelMap,
    events: mpsc::Sender<PeerEvent>,
) {
    let Ok(label) = channel.label().await else {
        return;
    };
    if !matches!(label.as_str(), "screen" | "control") {
        return;
    }
    channels.lock().await.insert(label.clone(), channel.clone());
    tokio::spawn(async move {
        while let Some(event) = channel.poll().await {
            if events
                .send(PeerEvent {
                    label: label.clone(),
                    event,
                })
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

async fn wait_for_gathering(gathered: Arc<Notify>) -> Result<(), String> {
    tokio::time::timeout(Duration::from_secs(20), gathered.notified())
        .await
        .map_err(|_| "ICE gathering timed out; check firewall and STUN access".to_owned())
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::{PeerControlMessage, screen_data_channel_init};

    #[test]
    fn screen_channel_drops_lost_chunks_instead_of_retransmitting_stale_frames() {
        let init = screen_data_channel_init();
        assert!(!init.ordered);
        assert_eq!(init.max_retransmits, Some(0));
        assert_eq!(init.max_packet_life_time, None);
    }

    #[test]
    fn input_ack_reports_sequence_and_dispatch_result() {
        let message = PeerControlMessage::Ack {
            sequence: 17,
            applied: true,
        };
        let bytes = serde_json::to_vec(&message).unwrap();
        assert!(matches!(
            serde_json::from_slice::<PeerControlMessage>(&bytes).unwrap(),
            PeerControlMessage::Ack {
                sequence: 17,
                applied: true
            }
        ));
    }
}
