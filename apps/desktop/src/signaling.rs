use std::{
    thread,
    time::{Duration, Instant},
};

use futures_util::{SinkExt, StreamExt};
use sparkdesk_protocol::{
    ClientMessage, Envelope, PROTOCOL_VERSION, ServerMessage, SignalPayload, encode_hex,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{sync::mpsc, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    AppState, NetworkState, NoticeState, SessionRole, identity::DeviceIdentity, input::RemoteInput,
    peer::PeerSession,
};

const SESSION_SETUP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub enum Command {
    Connect(String),
    Decide { accepted: bool },
    Disconnect,
    SetRemoteControl(bool),
    RemoteInput { sequence: u32, input: RemoteInput },
}

struct ActiveSession {
    request_id: String,
    remote_id: String,
    initiator_id: String,
    peer: Option<PeerSession>,
    setup_deadline: Instant,
}

struct PendingIncoming {
    request_id: String,
    remote_id: String,
    expires_at: Instant,
}

pub fn start(
    endpoint: String,
    identity: DeviceIdentity,
    app: AppHandle,
    commands: mpsc::UnboundedReceiver<Command>,
) {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                set_notice(&app, NoticeState::Error, error.to_string());
                return;
            }
        };
        runtime.block_on(async move {
            let mut commands = commands;
            let mut delay = Duration::from_secs(1);
            loop {
                match connect_and_wait(&endpoint, &identity, &app, &mut commands).await {
                    Ok(()) => set_notice(
                        &app,
                        NoticeState::Error,
                        "Signaling server closed the connection".into(),
                    ),
                    Err(error) => {
                        app.state::<AppState>()
                            .control_allowed
                            .store(false, std::sync::atomic::Ordering::Release);
                        set_session_role(&app, SessionRole::None);
                        set_notice(&app, NoticeState::Error, error);
                    }
                }
                set_network_state(&app, NetworkState::Reconnecting);
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        });
    });
}

async fn connect_and_wait(
    endpoint: &str,
    identity: &DeviceIdentity,
    app: &AppHandle,
    commands: &mut mpsc::UnboundedReceiver<Command>,
) -> Result<(), String> {
    if !(endpoint.starts_with("ws://") || endpoint.starts_with("wss://"))
        || endpoint.contains('?')
        || endpoint.contains('#')
    {
        return Err("Set a base ws:// or wss:// signaling URL without parameters".into());
    }
    let separator = if endpoint.ends_with('/') { "" } else { "/" };
    let url = format!(
        "{endpoint}{separator}v1/signal?device_id={}",
        identity.device_id()
    );
    set_network_state(app, NetworkState::Connecting);
    let (mut socket, _) = connect_async(url)
        .await
        .map_err(|error| error.to_string())?;
    let mut active: Option<ActiveSession> = None;
    let mut pending_incoming: Option<PendingIncoming> = None;
    let mut authenticated = false;
    let mut connection_poll = tokio::time::interval(Duration::from_millis(300));

    loop {
        tokio::select! {
            command = commands.recv(), if authenticated => {
                let Some(command) = command else { return Ok(()); };
                match command {
                    Command::Connect(target_id) => {
                        if active.is_some() {
                            set_notice(app, NoticeState::Error, "End the active session before starting another".into());
                            continue;
                        }
                        let request_id = random_id()?;
                        send_message(&mut socket, ClientMessage::JoinRequest { request_id: request_id.clone(), target_id: target_id.clone() }).await?;
                        active = Some(ActiveSession {
                            request_id,
                            remote_id: target_id,
                            initiator_id: identity.device_id().to_owned(),
                            peer: None,
                            setup_deadline: Instant::now() + SESSION_SETUP_TIMEOUT,
                        });
                        set_session_role(app, SessionRole::Viewer);
                        set_notice(app, NoticeState::Pending, "Sending connection request…".into());
                    }
                    Command::Decide { accepted } => {
                        if let Some(incoming) = pending_incoming.take() {
                            send_message(&mut socket, ClientMessage::JoinDecision { request_id: incoming.request_id.clone(), target_id: incoming.remote_id.clone(), accepted }).await?;
                            if accepted {
                                active = Some(ActiveSession {
                                    request_id: incoming.request_id,
                                    remote_id: incoming.remote_id.clone(),
                                    initiator_id: incoming.remote_id,
                                    peer: None,
                                    setup_deadline: Instant::now() + SESSION_SETUP_TIMEOUT,
                                });
                                set_session_role(app, SessionRole::Host);
                                set_notice(app, NoticeState::Pending, "Request accepted; establishing a direct connection…".into());
                            } else {
                                set_notice(app, NoticeState::Idle, "Connection request declined".into());
                            }
                        }
                    }
                    Command::Disconnect => {
                        app.state::<AppState>().control_allowed.store(false, std::sync::atomic::Ordering::Release);
                        if let Some(session) = active.take() {
                            if let Some(peer) = session.peer { peer.close().await; }
                            send_message(&mut socket, ClientMessage::EndSession { request_id: session.request_id, target_id: session.remote_id }).await?;
                            set_notice(app, NoticeState::Idle, "Session ended".into());
                        }
                        set_session_role(app, SessionRole::None);
                    }
                    Command::SetRemoteControl(enabled) => {
                        if let Some(peer) = active.as_ref().and_then(|session| session.peer.as_ref()) {
                            peer.send_permission(enabled).await?;
                        }
                    }
                    Command::RemoteInput { sequence, input } => {
                        if let Some(peer) = active.as_ref().and_then(|session| session.peer.as_ref()) {
                            peer.send_input(sequence, input).await?;
                        }
                    }
                }
            }
            frame = socket.next() => {
                let Some(frame) = frame else { return Err("Signaling server closed the connection".into()); };
                let frame = frame.map_err(|error| error.to_string())?;
                let Message::Text(text) = frame else { continue; };
                let envelope: Envelope<ServerMessage> = serde_json::from_str(&text).map_err(|_| "Signaling server sent an invalid message")?;
                if envelope.version != PROTOCOL_VERSION { return Err("Protocol version mismatch".into()); }
                match envelope.message {
                    ServerMessage::Challenge { nonce } => {
                        let nonce = decode_hex(&nonce).ok_or("Invalid server challenge")?;
                        let message = ClientMessage::Authenticate {
                            device_id: identity.device_id().to_owned(),
                            public_key: identity.public_key_hex(),
                            signature: encode_hex(&identity.sign_registration_challenge(&nonce).to_bytes()),
                        };
                        send_message(&mut socket, message).await?;
                    }
                    ServerMessage::Authenticated { device_id } if device_id == identity.device_id() => {
                        authenticated = true;
                        set_network_state(app, NetworkState::Online);
                    }
                    ServerMessage::Authenticated { .. } => return Err("Server returned a different device ID".into()),
                    ServerMessage::IncomingJoin { request_id, from_id } => {
                        if active.is_none() && pending_incoming.is_none() {
                            pending_incoming = Some(PendingIncoming {
                                request_id,
                                remote_id: from_id.clone(),
                                expires_at: Instant::now() + SESSION_SETUP_TIMEOUT,
                            });
                            set_notice(app, NoticeState::IncomingRequest, from_id);
                        }
                    }
                    ServerMessage::JoinStatus { request_id, target_id, online } => {
                        if active.as_ref().is_some_and(|session| session.request_id == request_id && session.remote_id == target_id) {
                            if online { set_notice(app, NoticeState::Pending, "Request delivered; waiting for the other user to accept…".into()); }
                            else { active = None; set_session_role(app, SessionRole::None); set_notice(app, NoticeState::Error, "That device is offline".into()); }
                        }
                    }
                    ServerMessage::JoinDecision { request_id, from_id, accepted } => {
                        if let Some(session) = &mut active {
                            if session.request_id == request_id && session.remote_id == from_id {
                                if accepted {
                                    let (peer, offer) = PeerSession::offer().await?;
                                    send_message(&mut socket, ClientMessage::Signal { request_id, target_id: from_id, payload: SignalPayload::Offer { sdp: offer } }).await?;
                                    session.peer = Some(peer);
                                    set_notice(app, NoticeState::Pending, "Creating a secure direct connection…".into());
                                } else { active = None; set_session_role(app, SessionRole::None); set_notice(app, NoticeState::Error, "Connection request declined".into()); }
                            }
                        }
                    }
                    ServerMessage::Signal { request_id, from_id, payload } => {
                        if let Some(active_session) = &mut active {
                            if active_session.request_id == request_id && active_session.remote_id == from_id {
                                match payload {
                                    SignalPayload::Offer { sdp } => {
                                        let (mut peer, answer) = PeerSession::answer(&sdp).await?;
                                        send_message(&mut socket, ClientMessage::Signal { request_id, target_id: from_id, payload: SignalPayload::Answer { sdp: answer } }).await?;
                                        peer.authenticate(
                                            identity,
                                            identity.device_id(),
                                            &active_session.remote_id,
                                            &active_session.initiator_id,
                                            identity.device_id(),
                                            &active_session.request_id,
                                        ).await?;
                                        peer.start_data_plane(
                                            true,
                                            app.clone(),
                                            app.state::<AppState>().control_allowed.clone(),
                                        ).await?;
                                        active_session.peer = Some(peer);
                                        set_notice(app, NoticeState::Pending, "Negotiating direct connection…".into());
                                    }
                                    SignalPayload::Answer { sdp } => if let Some(peer) = &mut active_session.peer {
                                        peer.set_answer(&sdp).await?;
                                        let target_id = if active_session.initiator_id == identity.device_id() {
                                            &active_session.remote_id
                                        } else {
                                            identity.device_id()
                                        };
                                        peer.authenticate(
                                            identity,
                                            identity.device_id(),
                                            &active_session.remote_id,
                                            &active_session.initiator_id,
                                            target_id,
                                            &active_session.request_id,
                                        ).await?;
                                        peer.start_data_plane(
                                            false,
                                            app.clone(),
                                            app.state::<AppState>().control_allowed.clone(),
                                        ).await?;
                                    },
                                    SignalPayload::IceCandidate { .. } => {}
                                }
                            }
                        }
                    }
                    ServerMessage::SessionEnded { request_id, .. } => {
                        if active.as_ref().is_some_and(|session| session.request_id == request_id) {
                            app.state::<AppState>().control_allowed.store(false, std::sync::atomic::Ordering::Release);
                            active = None;
                            set_session_role(app, SessionRole::None);
                            set_notice(app, NoticeState::Idle, "Session ended".into());
                        }
                    }
                    ServerMessage::Error { .. } => {
                        if let Some(session) = active.take() {
                            app.state::<AppState>().control_allowed.store(false, std::sync::atomic::Ordering::Release);
                            if let Some(peer) = session.peer { peer.close().await; }
                            set_session_role(app, SessionRole::None);
                        }
                        pending_incoming = None;
                        set_notice(app, NoticeState::Error, "Signaling server rejected the request".into());
                    }
                }
            }
            _ = connection_poll.tick() => {}
        }
        let now = Instant::now();
        if pending_incoming
            .as_ref()
            .is_some_and(|incoming| now >= incoming.expires_at)
        {
            pending_incoming = None;
            set_notice(
                app,
                NoticeState::Idle,
                "The connection request expired".into(),
            );
        }
        if active.as_ref().is_some_and(|session| {
            let connected = session.peer.as_ref().is_some_and(PeerSession::is_connected);
            setup_timed_out(session.setup_deadline, connected, now)
        }) {
            if let Some(session) = active.take() {
                app.state::<AppState>()
                    .control_allowed
                    .store(false, std::sync::atomic::Ordering::Release);
                if let Some(peer) = session.peer {
                    peer.close().await;
                }
                let _ = send_message(
                    &mut socket,
                    ClientMessage::EndSession {
                        request_id: session.request_id,
                        target_id: session.remote_id,
                    },
                )
                .await;
            }
            set_session_role(app, SessionRole::None);
            set_notice(
                app,
                NoticeState::Error,
                "The connection timed out. Try again.".into(),
            );
        }
        let peer_failed = active
            .as_ref()
            .and_then(|session| session.peer.as_ref())
            .is_some_and(PeerSession::is_failed);
        if peer_failed {
            if let Some(session) = active.take() {
                app.state::<AppState>()
                    .control_allowed
                    .store(false, std::sync::atomic::Ordering::Release);
                if let Some(peer) = session.peer {
                    peer.close().await;
                }
                send_message(
                    &mut socket,
                    ClientMessage::EndSession {
                        request_id: session.request_id,
                        target_id: session.remote_id,
                    },
                )
                .await?;
            }
            set_session_role(app, SessionRole::None);
            set_notice(app, NoticeState::Error, "Direct connection was lost".into());
        }
        if let Some(session) = &active {
            if session.peer.as_ref().is_some_and(PeerSession::is_connected) {
                set_notice(
                    app,
                    NoticeState::Connected,
                    "Direct connection established".into(),
                );
            }
        }
    }
}

async fn send_message<S>(socket: &mut S, message: ClientMessage) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let text = serde_json::to_string(&Envelope {
        version: PROTOCOL_VERSION,
        message,
    })
    .map_err(|error| error.to_string())?;
    timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(text.into())),
    )
    .await
    .map_err(|_| "Signaling send timed out".to_owned())?
    .map_err(|error| error.to_string())
}

fn random_id() -> Result<String, String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| error.to_string())?;
    Ok(encode_hex(&bytes))
}

fn setup_timed_out(deadline: Instant, connected: bool, now: Instant) -> bool {
    !connected && now >= deadline
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_value(pair[0])? << 4) | hex_value(pair[1])?))
        .collect()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn update_status(app: &AppHandle, update: impl FnOnce(&mut crate::AppStatus)) {
    let state = app.state::<AppState>();
    let payload = if let Ok(mut status) = state.status.lock() {
        let previous = status.clone();
        update(&mut status);
        (*status != previous).then(|| status.clone())
    } else {
        None
    };
    if let Some(payload) = payload {
        let _ = app.emit("app-status", payload);
    }
}

fn set_network_state(app: &AppHandle, network_state: NetworkState) {
    update_status(app, |status| status.network_state = network_state);
}

fn set_session_role(app: &AppHandle, role: SessionRole) {
    app.state::<AppState>()
        .control_allowed
        .store(false, std::sync::atomic::Ordering::Release);
    update_status(app, |status| {
        status.session_role = role;
        status.control_enabled = false;
    });
}

pub(crate) fn set_control_status(app: &AppHandle, enabled: bool) {
    update_status(app, |status| {
        if status.session_role == SessionRole::Viewer {
            status.control_enabled = enabled;
        }
    });
}

fn set_notice(app: &AppHandle, notice_state: NoticeState, detail: String) {
    update_status(app, |status| {
        status.notice_state = notice_state;
        status.detail = detail;
    });
}

#[cfg(test)]
mod tests {
    use super::setup_timed_out;
    use std::time::{Duration, Instant};

    #[test]
    fn setup_timeout_expires_only_unconnected_sessions() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(60);

        assert!(!setup_timed_out(deadline, false, now));
        assert!(setup_timed_out(deadline, false, deadline));
        assert!(!setup_timed_out(
            deadline,
            true,
            deadline + Duration::from_secs(1)
        ));
    }
}
