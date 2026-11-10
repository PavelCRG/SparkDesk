use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;

pub mod identity;
mod input;
mod peer;
mod screen;
mod signaling;

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub device_id: String,
    pub network_state: NetworkState,
    pub notice_state: NoticeState,
    pub session_role: SessionRole,
    pub control_enabled: bool,
    pub detail: String,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    None,
    Host,
    Viewer,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkState {
    NotConfigured,
    Connecting,
    Online,
    Reconnecting,
}

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeState {
    Idle,
    IncomingRequest,
    Error,
    Pending,
    Connected,
}

pub struct AppState {
    pub device_id: String,
    pub commands: mpsc::UnboundedSender<signaling::Command>,
    pub signaling_configured: bool,
    pub status: Mutex<AppStatus>,
    pub control_allowed: Arc<AtomicBool>,
    input_rate_limit: Mutex<input::InputRateLimit>,
}

#[tauri::command]
fn get_app_status(state: tauri::State<'_, AppState>) -> AppStatus {
    state
        .status
        .lock()
        .expect("app status lock poisoned")
        .clone()
}

#[tauri::command]
fn copy_device_id(state: tauri::State<'_, AppState>) -> Result<(), String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(&state.device_id))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn connect(peer_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let target = peer_id.trim().to_ascii_uppercase();
    if target.len() != 24 || !target.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Enter a valid 24-character device ID".into());
    }
    if target == state.device_id {
        return Err("You cannot connect to this device".into());
    }
    if !state.signaling_configured {
        return Err("Set SPARKDESK_SIGNALING_URL before starting SparkDesk".into());
    }
    state
        .commands
        .send(signaling::Command::Connect(target))
        .map_err(|_| "Signaling service is unavailable".into())
}

#[tauri::command]
fn decide(accepted: bool, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .commands
        .send(signaling::Command::Decide { accepted })
        .map_err(|_| "Signaling service is unavailable".into())
}

#[tauri::command]
fn disconnect(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.control_allowed.store(false, Ordering::Release);
    state
        .commands
        .send(signaling::Command::Disconnect)
        .map_err(|_| "Signaling service is unavailable".into())
}

#[tauri::command]
fn set_remote_control(
    enabled: bool,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut status = state.status.lock().map_err(|_| "app status unavailable")?;
    if !can_toggle_remote_control(&status) {
        return Err("Only the connected host can change remote control permission".into());
    }
    state
        .commands
        .send(signaling::Command::SetRemoteControl(enabled))
        .map_err(|_| "Signaling service is unavailable")?;
    state.control_allowed.store(enabled, Ordering::Release);
    status.control_enabled = enabled;
    let payload = status.clone();
    drop(status);
    let _ = app.emit("app-status", payload);
    Ok(())
}

#[tauri::command]
fn send_remote_input(
    input: input::RemoteInput,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let status = state.status.lock().map_err(|_| "app status unavailable")?;
    if !can_send_remote_input(&status) {
        return Err("The host has not enabled remote control".into());
    }
    input::validate_input(&input).map_err(str::to_owned)?;
    if !state
        .input_rate_limit
        .lock()
        .map_err(|_| "input rate limiter unavailable")?
        .allow(Instant::now())
    {
        return Err("Remote input rate limit exceeded".into());
    }
    let payload = serde_json::to_vec(&input).map_err(|error| error.to_string())?;
    if payload.len() > 256 {
        return Err("Remote input is too large".into());
    }
    state
        .commands
        .send(signaling::Command::RemoteInput(input))
        .map_err(|_| "Signaling service is unavailable".into())
}

fn can_toggle_remote_control(status: &AppStatus) -> bool {
    status.session_role == SessionRole::Host && status.notice_state == NoticeState::Connected
}

fn can_send_remote_input(status: &AppStatus) -> bool {
    status.session_role == SessionRole::Viewer
        && status.notice_state == NoticeState::Connected
        && status.control_enabled
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let profile = std::env::args()
        .find_map(|argument| argument.strip_prefix("--profile=").map(str::to_owned));
    let identity = identity::load_or_create(profile.as_deref())?;
    let device_id = identity.device_id().to_owned();
    let endpoint = std::env::var("SPARKDESK_SIGNALING_URL").ok();
    let signaling_configured = endpoint.is_some();
    let (commands, command_rx) = mpsc::unbounded_channel();
    let state = AppState {
        device_id: device_id.clone(),
        commands,
        signaling_configured,
        status: Mutex::new(AppStatus {
            device_id,
            network_state: if signaling_configured {
                NetworkState::Connecting
            } else {
                NetworkState::NotConfigured
            },
            notice_state: NoticeState::Idle,
            session_role: SessionRole::None,
            control_enabled: false,
            detail: String::new(),
        }),
        control_allowed: Arc::new(AtomicBool::new(false)),
        input_rate_limit: Mutex::new(input::InputRateLimit::new(Instant::now())),
    };

    tauri::Builder::default()
        .manage(state)
        .setup(move |app| {
            if let Some(endpoint) = endpoint {
                signaling::start(endpoint, identity, app.handle().clone(), command_rx);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            copy_device_id,
            connect,
            decide,
            disconnect,
            set_remote_control,
            send_remote_input
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        AppStatus, NetworkState, NoticeState, SessionRole, can_send_remote_input,
        can_toggle_remote_control,
    };

    #[test]
    fn app_status_uses_named_states_in_the_frontend_contract() {
        let status = AppStatus {
            device_id: "0123456789ABCDEF01234567".into(),
            network_state: NetworkState::Online,
            notice_state: NoticeState::IncomingRequest,
            session_role: SessionRole::None,
            control_enabled: false,
            detail: String::new(),
        };

        let value = serde_json::to_value(status).unwrap();
        assert_eq!(value["networkState"], "online");
        assert_eq!(value["noticeState"], "incoming_request");
    }

    #[test]
    fn only_connected_host_can_grant_control_and_only_enabled_viewer_can_send_input() {
        let mut status = AppStatus {
            device_id: String::new(),
            network_state: NetworkState::Online,
            notice_state: NoticeState::Connected,
            session_role: SessionRole::Host,
            control_enabled: false,
            detail: String::new(),
        };
        assert!(can_toggle_remote_control(&status));
        assert!(!can_send_remote_input(&status));

        status.session_role = SessionRole::Viewer;
        assert!(!can_toggle_remote_control(&status));
        assert!(!can_send_remote_input(&status));
        status.control_enabled = true;
        assert!(can_send_remote_input(&status));
        status.notice_state = NoticeState::Idle;
        assert!(!can_send_remote_input(&status));
    }
}
