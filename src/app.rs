use enigo::Enigo;
use MessAuto::{enter, paste};

slint::include_modules!();

pub fn main(code: &str, from_app: &str) -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;
    ui.set_code(code.into());
    ui.set_from_app(from_app.into());
    let mut enigo = Enigo::new();

    let ui_handle = ui.as_weak();
    ui.on_paste_code(move || {
        let ui = ui_handle.unwrap();
        paste(&mut enigo);
        enter(&mut enigo);
        ui.hide().unwrap();
    });

    ui.run()
}
