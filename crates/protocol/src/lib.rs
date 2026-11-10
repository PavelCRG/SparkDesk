use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_SDP_BYTES: usize = 16 * 1024;
pub const MAX_ICE_CANDIDATE_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub version: u16,
    pub message: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMessage {
    Authenticate {
        device_id: String,
        public_key: String,
        signature: String,
    },
    JoinRequest {
        request_id: String,
        target_id: String,
    },
    JoinDecision {
        request_id: String,
        target_id: String,
        accepted: bool,
    },
    Signal {
        request_id: String,
        target_id: String,
        payload: SignalPayload,
    },
    EndSession {
        request_id: String,
        target_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ServerMessage {
    Challenge {
        nonce: String,
    },
    Authenticated {
        device_id: String,
    },
    IncomingJoin {
        request_id: String,
        from_id: String,
    },
    JoinStatus {
        request_id: String,
        target_id: String,
        online: bool,
    },
    JoinDecision {
        request_id: String,
        from_id: String,
        accepted: bool,
    },
    Signal {
        request_id: String,
        from_id: String,
        payload: SignalPayload,
    },
    SessionEnded {
        request_id: String,
        from_id: String,
    },
    Error {
        code: ErrorCode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SignalPayload {
    Offer { sdp: String },
    Answer { sdp: String },
    IceCandidate { candidate: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidMessage,
    InvalidIdentity,
    TargetOffline,
    RequestNotFound,
    SessionRejected,
    UnsupportedVersion,
}

impl ClientMessage {
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Authenticate {
                device_id,
                public_key,
                signature,
            } => {
                if !is_hex(device_id, 24) || !is_hex(public_key, 64) || !is_hex(signature, 128) {
                    return Err("invalid identity field");
                }
                if device_id_from_public_key_hex(public_key).as_deref() != Some(device_id) {
                    return Err("device ID does not match public key");
                }
            }
            Self::JoinRequest {
                request_id,
                target_id,
            }
            | Self::JoinDecision {
                request_id,
                target_id,
                ..
            }
            | Self::Signal {
                request_id,
                target_id,
                ..
            }
            | Self::EndSession {
                request_id,
                target_id,
            } => {
                if !is_hex(request_id, 32) || !is_hex(target_id, 24) {
                    return Err("invalid request or target ID");
                }
            }
        }

        if let Self::Signal { payload, .. } = self {
            match payload {
                SignalPayload::Offer { sdp } | SignalPayload::Answer { sdp }
                    if sdp.len() > MAX_SDP_BYTES =>
                {
                    return Err("SDP exceeds the message limit");
                }
                SignalPayload::IceCandidate { candidate }
                    if candidate.len() > MAX_ICE_CANDIDATE_BYTES =>
                {
                    return Err("ICE candidate exceeds the message limit");
                }
                _ => {}
            }
        }

        Ok(())
    }
}

pub fn device_id_from_public_key(public_key: &[u8; 32]) -> String {
    encode_hex(&public_key[..12])
}

pub fn device_id_from_public_key_hex(public_key: &str) -> Option<String> {
    if !is_hex(public_key, 64) {
        return None;
    }

    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let high = hex_value(public_key.as_bytes()[index * 2])?;
        let low = hex_value(public_key.as_bytes()[index * 2 + 1])?;
        *byte = high << 4 | low;
    }
    Some(device_id_from_public_key(&bytes))
}

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0F) as usize] as char);
    }
    encoded
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
