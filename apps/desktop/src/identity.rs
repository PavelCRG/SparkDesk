use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write as IoWrite},
    path::PathBuf,
};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use directories::ProjectDirs;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sparkdesk_protocol::{device_id_from_public_key, encode_hex};
use zeroize::Zeroizing;

const SECRET_KEY_BYTES: usize = 32;

pub struct DeviceIdentity {
    signing_key: SigningKey,
    device_id: String,
}

impl DeviceIdentity {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    pub fn public_key_hex(&self) -> String {
        encode_hex(&self.public_key())
    }

    pub fn sign_registration_challenge(&self, nonce: &[u8]) -> Signature {
        let mut message = b"SparkDesk signaling registration v1\0".to_vec();
        message.extend_from_slice(nonce);
        self.signing_key.sign(&message)
    }

    pub fn sign_peer_proof(&self, context: &[u8]) -> [u8; 64] {
        self.signing_key.sign(context).to_bytes()
    }
}

pub fn verify_peer_proof(
    expected_device_id: &str,
    public_key: [u8; 32],
    signature: [u8; 64],
    context: &[u8],
) -> bool {
    if device_id_from_public_key(&public_key) != expected_device_id {
        return false;
    }
    let Ok(key) = VerifyingKey::from_bytes(&public_key) else {
        return false;
    };
    key.verify_strict(context, &Signature::from_bytes(&signature))
        .is_ok()
}

pub fn load_or_create(profile: Option<&str>) -> io::Result<DeviceIdentity> {
    let path = identity_path(profile)?;
    fs::create_dir_all(path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "identity path has no parent")
    })?)?;

    let signing_key = match load_key(&path) {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => create_key(&path)?,
        Err(error) => return Err(error),
    };

    let device_id = device_id_from_public_key(&signing_key.verifying_key().to_bytes());
    Ok(DeviceIdentity {
        signing_key,
        device_id,
    })
}

fn identity_path(profile: Option<&str>) -> io::Result<PathBuf> {
    if profile.is_some_and(|name| {
        name.is_empty()
            || name.len() > 32
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "profile names must be 1-32 ASCII letters, numbers, hyphens or underscores",
        ));
    }
    ProjectDirs::from("com", "SparkDesk", "SparkDesk")
        .map(|dirs| {
            profile.map_or_else(
                || dirs.config_dir().join("device-key"),
                |name| {
                    dirs.config_dir()
                        .join("profiles")
                        .join(name)
                        .join("device-key")
                },
            )
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "user config directory unavailable"))
}

#[cfg(test)]
mod tests {
    use super::{identity_path, verify_peer_proof};
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn profile_path_is_separate_from_the_default_identity() {
        let default = identity_path(None).unwrap();
        let helper = identity_path(Some("helper")).unwrap();

        assert_ne!(default, helper);
        assert!(helper.ends_with("profiles/helper/device-key"));
    }

    #[test]
    fn profile_name_cannot_escape_its_directory() {
        assert!(identity_path(Some("../default")).is_err());
    }

    #[test]
    fn peer_proof_binds_the_claimed_device_id_to_its_key() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public_key = key.verifying_key().to_bytes();
        let device_id = sparkdesk_protocol::device_id_from_public_key(&public_key);
        let proof = key.sign(b"session context");

        assert!(verify_peer_proof(
            &device_id,
            public_key,
            proof.to_bytes(),
            b"session context"
        ));
        assert!(!verify_peer_proof(
            "000000000000000000000000",
            public_key,
            proof.to_bytes(),
            b"session context"
        ));
        assert!(!verify_peer_proof(
            &device_id,
            public_key,
            proof.to_bytes(),
            b"different session"
        ));
    }
}

fn load_key(path: &PathBuf) -> io::Result<SigningKey> {
    let mut file = File::open(path)?;
    let mut secret = Zeroizing::new([0u8; SECRET_KEY_BYTES]);
    if let Err(error) = file.read_exact(&mut *secret) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("device key must be exactly {SECRET_KEY_BYTES} bytes: {error}"),
        ));
    }

    let mut extra = [0u8; 1];
    if file.read(&mut extra)? != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("device key must be exactly {SECRET_KEY_BYTES} bytes"),
        ));
    }

    Ok(SigningKey::from_bytes(&secret))
}

fn create_key(path: &PathBuf) -> io::Result<SigningKey> {
    let mut secret = Zeroizing::new([0u8; SECRET_KEY_BYTES]);
    getrandom::fill(&mut *secret).map_err(|error| io::Error::other(error.to_string()))?;
    let key = SigningKey::from_bytes(&secret);

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    match options.open(path) {
        Ok(mut file) => {
            file.write_all(&key.to_bytes())?;
            file.sync_all()?;
            Ok(key)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => load_key(path),
        Err(error) => Err(error),
    }
}
