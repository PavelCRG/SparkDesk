pub mod identity;
mod signaling;

slint::include_modules!();

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = MainWindow::new()?;
    let identity = identity::load_or_create()?;
    let device_id = identity.device_id().to_owned();
    window.set_device_id(device_id.clone().into());

    if let Ok(endpoint) = std::env::var("SPARKDESK_SIGNALING_URL") {
        window.set_network_status("Подключаемся…".into());
        signaling::start(endpoint, identity, window.as_weak());
    }

    let weak_window = window.as_weak();
    window.on_copy_id(move || {
        let result =
            arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(&device_id));

        if let Some(window) = weak_window.upgrade() {
            window.set_notice(match result {
                Ok(()) => "ID скопирован".into(),
                Err(_) => "Не удалось скопировать ID".into(),
            });
        }
    });

    let weak_window = window.as_weak();
    window.on_connect(move || {
        if let Some(window) = weak_window.upgrade() {
            window.set_notice("Подключение пока не настроено".into());
        }
    });

    window.run()?;
    Ok(())
}
