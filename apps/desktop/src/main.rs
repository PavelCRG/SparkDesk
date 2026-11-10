pub mod identity;
mod signaling;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = MainWindow::new()?;
    let identity = identity::load_or_create()?;
    let device_id = identity.device_id().to_owned();
    window.set_device_id(device_id.clone().into());

    if let Ok(endpoint) = std::env::var("SPARKDESK_SIGNALING_URL") {
        window.set_network_state(1);
        signaling::start(endpoint, identity, window.as_weak());
    }

    let copy_device_id = device_id.clone();
    let weak_window = window.as_weak();
    window.on_copy_id(move || {
        let result =
            arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&copy_device_id));

        if let Some(window) = weak_window.upgrade() {
            window.set_notice_state(if result.is_ok() { 1 } else { 2 });
            window.set_notice_detail("".into());
        }
    });

    let weak_window = window.as_weak();
    window.on_connect(move |peer_id| {
        let peer_id = peer_id.trim();
        let valid_id = peer_id.len() == 24 && peer_id.bytes().all(|byte| byte.is_ascii_hexdigit());
        let notice_state = if !valid_id {
            6
        } else if peer_id.eq_ignore_ascii_case(&device_id) {
            7
        } else {
            5
        };
        if let Some(window) = weak_window.upgrade() {
            window.set_notice_state(notice_state);
            window.set_notice_detail("".into());
        }
    });

    window.run()?;
    Ok(())
}
