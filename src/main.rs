use tao::platform::macos::ActivationPolicy;
use tao::{
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::macos::EventLoopExtMacOS,
};
use tray_icon::{menu::MenuEvent, TrayEvent};
use MessAuto::{
    auto_launch, auto_thread, check_accessibility, check_full_disk_access, get_current_exe_path,
    get_sys_locale, read_config, Config, TrayIcon, TrayMenu, TrayMenuItems,
};
fn main() {
    println!("{:?}", get_current_exe_path());

    let locale = get_sys_locale();
    rust_i18n::set_locale(locale);
    check_full_disk_access();
    let mut event_loop = EventLoopBuilder::new().build();
    // set eventloop policy to hide dock icon
    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let auto = auto_launch();

    let mut config: Config = read_config();
    auto_thread();

    let tray_menu_items = TrayMenuItems::build(&config);
    let tray_menu = TrayMenu::build(&tray_menu_items);
    let mut tray_icon = TrayIcon::build(tray_menu);

    // check visible
    if config.hide_icon_forever {
        tray_icon.as_mut().unwrap().set_visible(false);
    } else {
        tray_icon.as_mut().unwrap().set_visible(true);
    }

    let menu_channel = MenuEvent::receiver();
    let tray_channel = TrayEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        if let Ok(event) = menu_channel.try_recv() {
            if event.id == tray_menu_items.quit_i.id() {
                tray_icon.take();
                *control_flow = ControlFlow::Exit;
            } else if event.id == tray_menu_items.check_hide_icon_for_now.id() {
                tray_icon.as_mut().unwrap().set_visible(false);
            } else if event.id == tray_menu_items.check_hide_icon_forever.id() {
                config.hide_icon_forever = true;
                tray_icon.as_mut().unwrap().set_visible(false);
                config.update();
            } else if event.id == tray_menu_items.check_auto_paste.id() {
                if tray_menu_items.check_auto_paste.is_checked() {
                    if check_accessibility() {
                        config.auto_paste = true;
                        tray_menu_items
                            .check_auto_paste
                            .set_checked(config.auto_paste);
                        tray_menu_items
                            .check_auto_return
                            .set_enabled(config.auto_paste);
                        config.update();
                    } else {
                        config.auto_paste = false;
                        tray_menu_items
                            .check_auto_paste
                            .set_checked(config.auto_paste);
                    }
                } else {
                    config.auto_paste = false;
                    config.auto_return = false;
                    tray_menu_items.check_auto_return.set_enabled(false);
                    tray_menu_items.check_auto_return.set_checked(false);
                    config.update();
                }
            } else if event.id == tray_menu_items.check_auto_return.id() {
                if tray_menu_items.check_auto_return.is_checked() {
                    config.auto_return = true;
                    config.update();
                } else {
                    config.auto_return = false;
                    config.update();
                }
            } else if event.id == tray_menu_items.check_launch_at_login.id() {
                if tray_menu_items.check_launch_at_login.is_checked() {
                    auto.enable().is_ok();
                    if auto.is_enabled().unwrap() {
                        config.launch_at_login = true;
                        config.update();
                    } else {
                        tray_menu_items.check_launch_at_login.set_checked(false);
                    }
                } else {
                    auto.disable().is_ok();
                    if !auto.is_enabled().unwrap() {
                        config.launch_at_login = false;
                        config.update();
                    } else {
                        tray_menu_items.check_launch_at_login.set_checked(true);
                    }
                }
            } else {
                println!("what have you done?!");
            }
        }
        if let Ok(event) = tray_channel.try_recv() {
            println!("{event:?}");
        }
    });
}
