use enigo::Enigo;
use log::info;
use rust_i18n::t;
use MessAuto::{enter, paste};

slint::include_modules!();

pub fn main(code: &str, from_app: &str) -> Result<(), slint::PlatformError> {
    let paste_code_instruction = t!("paste_code_instruction");
    let verification_code = t!("verification-code");
    let from_label = t!("from-label");
    let ui = AppWindow::new()?;
    ui.set_code(code.into());
    ui.set_from_app(from_app.into());
    ui.set_paste_code_instruction(paste_code_instruction.into());
    ui.set_verification_code(verification_code.into());
    ui.set_from_label(from_label.into());
    let mut enigo = Enigo::new();

    let ui_handle = ui.as_weak();
    ui.on_paste_code(move || {
        let ui = ui_handle.unwrap();
        paste(&mut enigo);
        info!("执行粘贴验证码");
        enter(&mut enigo);
        info!("执行回车");
        ui.hide().unwrap();
    });

    ui.run()
}
