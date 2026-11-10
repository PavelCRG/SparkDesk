use std::{thread, time::Duration};

use futures_util::{SinkExt, StreamExt};
use sparkdesk_protocol::{
    ClientMessage, Envelope, PROTOCOL_VERSION, ServerMessage, SignalPayload, encode_hex,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::{sync::mpsc, time::timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{AppState, NetworkState, NoticeState, identity::DeviceIdentity, peer::PeerSession};

#[derive(Debug)]
pub enum Command {
    Connect(String),
    Decide { accepted: bool },
    Disconnect,
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
                    Err(error) => set_notice(&app, NoticeState::Error, error),
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
    let mut active: Option<(String, String, Option<PeerSession>)> = None;
    let mut pending_incoming: Option<(String, String)> = None;
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
                        active = Some((request_id, target_id, None));
                        set_notice(app, NoticeState::Pending, "Sending connection request…".into());
                    }
                    Command::Decide { accepted } => {
                        if let Some((request_id, target_id)) = pending_incoming.take() {
                            send_message(&mut socket, ClientMessage::JoinDecision { request_id: request_id.clone(), target_id: target_id.clone(), accepted }).await?;
                            if accepted {
                                active = Some((request_id, target_id, None));
                                set_notice(app, NoticeState::Pending, "Request accepted; establishing a direct connection…".into());
                            } else {
                                set_notice(app, NoticeState::Idle, "Connection request declined".into());
                            }
                        }
                    }
                    Command::Disconnect => {
                        if let Some((request_id, target_id, session)) = active.take() {
                            if let Some(session) = session { session.close().await; }
                            send_message(&mut socket, ClientMessage::EndSession { request_id, target_id }).await?;
                            set_notice(app, NoticeState::Idle, "Session ended".into());
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
                            pending_incoming = Some((request_id, from_id.clone()));
                            set_notice(app, NoticeState::IncomingRequest, from_id);
                        }
                    }
                    ServerMessage::JoinStatus { request_id, target_id, online } => {
                        if active.as_ref().is_some_and(|(id, peer, _)| id == &request_id && peer == &target_id) {
                            if online { set_notice(app, NoticeState::Pending, "Request delivered; waiting for the other user to accept…".into()); }
                            else { active = None; set_notice(app, NoticeState::Error, "That device is offline".into()); }
                        }
                    }
                    ServerMessage::JoinDecision { request_id, from_id, accepted } => {
                        if let Some((id, target, _)) = &active {
                            if id == &request_id && target == &from_id {
                                if accepted {
                                    let (session, offer) = PeerSession::offer().await?;
                                    send_message(&mut socket, ClientMessage::Signal { request_id, target_id: from_id, payload: SignalPayload::Offer { sdp: offer } }).await?;
                                    active = Some((id.clone(), target.clone(), Some(session)));
                                    set_notice(app, NoticeState::Pending, "Creating a secure direct connection…".into());
                                } else { active = None; set_notice(app, NoticeState::Error, "Connection request declined".into()); }
                            }
                        }
                    }
                    ServerMessage::Signal { request_id, from_id, payload } => {
                        if let Some((id, target, session)) = &active {
                            if id == &request_id && target == &from_id {
                                match payload {
                                    SignalPayload::Offer { sdp } => {
                                        let (session, answer) = PeerSession::answer(&sdp).await?;
                                        send_message(&mut socket, ClientMessage::Signal { request_id, target_id: from_id, payload: SignalPayload::Answer { sdp: answer } }).await?;
                                        active = Some((id.clone(), target.clone(), Some(session)));
                                        set_notice(app, NoticeState::Pending, "Negotiating direct connection…".into());
                                    }
                                    SignalPayload::Answer { sdp } => if let Some(session) = session { session.set_answer(&sdp).await?; },
                                    SignalPayload::IceCandidate { .. } => {}
                                }
                            }
                        }
                    }
                    ServerMessage::SessionEnded { request_id, .. } => {
                        if active.as_ref().is_some_and(|(id, _, _)| id == &request_id) { active = None; set_notice(app, NoticeState::Idle, "Session ended".into()); }
                    }
                    ServerMessage::Error { .. } => set_notice(app, NoticeState::Error, "Signaling server rejected the request".into()),
                }
            }
            _ = connection_poll.tick() => {}
        }
        if let Some((_, _, Some(session))) = &active {
            if session.is_connected() {
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

fn set_notice(app: &AppHandle, notice_state: NoticeState, detail: String) {
    update_status(app, |status| {
        status.notice_state = notice_state;
        status.detail = detail;
    });
}
