use std::{thread, time::Duration};

use futures_util::{SinkExt, StreamExt};
use sparkdesk_protocol::{ClientMessage, Envelope, PROTOCOL_VERSION, ServerMessage, encode_hex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{MainWindow, identity::DeviceIdentity};

pub fn start(endpoint: String, identity: DeviceIdentity, window: slint::Weak<MainWindow>) {
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                set_status(&window, format!("Ошибка: {error}"));
                return;
            }
        };

        runtime.block_on(async move {
            let mut delay = Duration::from_secs(1);
            loop {
                match connect_and_wait(&endpoint, &identity, &window).await {
                    Ok(()) => set_status(&window, "Соединение закрыто".into()),
                    Err(error) => set_status(&window, format!("Нет связи: {error}")),
                }
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(Duration::from_secs(30));
            }
        });
    });
}

async fn connect_and_wait(
    endpoint: &str,
    identity: &DeviceIdentity,
    window: &slint::Weak<MainWindow>,
) -> Result<(), String> {
    if !(endpoint.starts_with("ws://") || endpoint.starts_with("wss://"))
        || endpoint.contains('?')
        || endpoint.contains('#')
    {
        return Err("укажите базовый ws:// или wss:// адрес без параметров".into());
    }

    let separator = if endpoint.ends_with('/') { "" } else { "/" };
    let path_has_endpoint = endpoint.ends_with("/v1/signal");
    let url = if path_has_endpoint {
        format!("{endpoint}?device_id={}", identity.device_id())
    } else {
        format!(
            "{endpoint}{separator}v1/signal?device_id={}",
            identity.device_id()
        )
    };

    set_status(window, "Подключаемся…".into());
    let (mut socket, _) = connect_async(url)
        .await
        .map_err(|error| error.to_string())?;

    while let Some(frame) = socket.next().await {
        let frame = frame.map_err(|error| error.to_string())?;
        let text = match frame {
            Message::Text(text) => text,
            Message::Close(_) => return Err("сервер закрыл соединение".into()),
            _ => continue,
        };

        let envelope: Envelope<ServerMessage> =
            serde_json::from_str(&text).map_err(|_| "сервер прислал неверное сообщение")?;
        if envelope.version != PROTOCOL_VERSION {
            return Err("версия протокола не совпадает".into());
        }

        match envelope.message {
            ServerMessage::Challenge { nonce } => {
                let nonce = decode_hex(&nonce).ok_or("неверный challenge сервера")?;
                let signature = identity.sign_registration_challenge(&nonce);
                let message = ClientMessage::Authenticate {
                    device_id: identity.device_id().to_owned(),
                    public_key: identity.public_key_hex(),
                    signature: encode_hex(&signature.to_bytes()),
                };
                let text = serde_json::to_string(&Envelope {
                    version: PROTOCOL_VERSION,
                    message,
                })
                .map_err(|error| error.to_string())?;
                socket
                    .send(Message::Text(text.into()))
                    .await
                    .map_err(|error| error.to_string())?;
            }
            ServerMessage::Authenticated { device_id } if device_id == identity.device_id() => {
                set_status(window, "В сети".into());
            }
            ServerMessage::Authenticated { .. } => return Err("сервер вернул другой ID".into()),
            ServerMessage::IncomingJoin { from_id, .. } => {
                set_notice(window, format!("Запрос на подключение от {from_id}"));
            }
            ServerMessage::Error { .. } => return Err("сервер отклонил сообщение".into()),
            _ => {}
        }
    }

    Err("сервер закрыл соединение".into())
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_value(pair[0])?;
            let low = hex_value(pair[1])?;
            Some(high << 4 | low)
        })
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

fn set_status(window: &slint::Weak<MainWindow>, value: String) {
    let window = window.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = window.upgrade() {
            window.set_network_status(value.into());
        }
    });
}

fn set_notice(window: &slint::Weak<MainWindow>, value: String) {
    let window = window.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = window.upgrade() {
            window.set_notice(value.into());
        }
    });
}
