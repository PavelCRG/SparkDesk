use std::sync::Mutex;

use serde::Serialize;
use tokio::sync::mpsc;

pub mod identity;
mod peer;
mod signaling;

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub device_id: String,
    pub network_state: i32,
    pub notice_state: i32,
    pub detail: String,
}

pub struct AppState {
    pub device_id: String,
    pub commands: mpsc::UnboundedSender<signaling::Command>,
    pub signaling_configured: bool,
    pub status: Mutex<AppStatus>,
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
    state
        .commands
        .send(signaling::Command::Disconnect)
        .map_err(|_| "Signaling service is unavailable".into())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let identity = identity::load_or_create()?;
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
            network_state: if signaling_configured { 1 } else { 0 },
            notice_state: 0,
            detail: String::new(),
        }),
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
            disconnect
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}
