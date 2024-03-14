use enigo::{Enigo, MouseControllable};
use i_slint_backend_winit::winit::{
    dpi::{LogicalPosition, Position},
    platform::macos::WindowBuilderExtMacOS,
};
use log::info;
use rust_i18n::t;
use MessAuto::{enter, get_sys_locale, paste};

slint::include_modules!();

pub fn main(code: &str, from_app: &str) -> Result<(), slint::PlatformError> {
    let locale = get_sys_locale();
    rust_i18n::set_locale(locale);

    let paste_code_instruction = t!("paste_code_instruction");
    let verification_code_label = format!(
        "{}: {}\n{} {}",
        t!("verification-code"),
        code,
        t!("from-label"),
        from_app
    );

    let mut backend = i_slint_backend_winit::Backend::new().unwrap();
    backend.window_builder_hook = Some(Box::new(|builder| {
        builder
            .with_titlebar_buttons_hidden(true)
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
    }));
    slint::platform::set_platform(Box::new(backend)).unwrap();

    let ui = AppWindow::new()?;

    let ui_weak = ui.as_weak();

    ui.on_mouse_move(move |delta_x, delta_y| {
        let ui_weak = ui_weak.unwrap();
        let logical_pos = ui_weak.window().position();
        ui_weak.window().set_position(slint::PhysicalPosition::new(
            logical_pos.x + delta_x as i32,
            logical_pos.y + delta_y as i32,
        ));
    });

    ui.set_paste_code_instruction(paste_code_instruction.to_string().into());
    ui.set_verification_code_label(verification_code_label.to_string().into());

    let mut enigo = Enigo::new();

    let mouse_pos = enigo.mouse_location();
    ui.window().set_position(
        slint::LogicalPosition::new(mouse_pos.0 as f32, mouse_pos.1 as f32)
            .to_physical(ui.window().scale_factor()),
    );

    let ui_handle = ui.as_weak();

    ui.on_paste_code(move || {
        let ui = ui_handle.unwrap();
        paste(&mut enigo);
        info!("{}", t!("paste-verification-code"));
        enter(&mut enigo);
        info!("{}", t!("press-enter"));
        ui.hide().unwrap();
    });

    ui.run()
}
