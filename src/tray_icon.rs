use std::fs::File;
use std::process::Command;
use std::sync::mpsc;

use log::{info, trace, warn};
use native_dialog::MessageDialog;
use rust_i18n::t;
use simplelog::{
    ColorChoice, CombinedLogger, Config, LevelFilter, TermLogger, TerminalMode, WriteLogger,
};
use tao::platform::macos::ActivationPolicy;
use tao::{
    event_loop::{ControlFlow, EventLoopBuilder},
    platform::macos::EventLoopExtMacOS,
};
use tray_icon::{menu::MenuEvent, TrayIconEvent};

use MessAuto::{
    auto_launch, check_accessibility, check_full_disk_access, config_path, get_current_exe_path,
    get_sys_locale, log_path, mail_thread, messages_thread, read_config, replace_old_version,
    update_thread, TrayIcon, TrayMenu, TrayMenuItems,
};

rust_i18n::i18n!("locales");
pub fn main() {
    CombinedLogger::init(vec![
        TermLogger::new(
            LevelFilter::Info,
            Config::default(),
            TerminalMode::Mixed,
            ColorChoice::Auto,
        ),
        WriteLogger::new(
            LevelFilter::Info,
            Config::default(),
            File::create(log_path()).unwrap(),
        ),
    ])
    .unwrap();
    info!("{}", t!("log-initialization-completed"));
    let locale = get_sys_locale();
    info!("{}: {}", t!("detect-and-set-app-language-to"), locale);
    rust_i18n::set_locale(locale);
    check_full_disk_access();
    info!("{}", t!("successfully-obtained-disk-access-permissions"));
    let mut event_loop = EventLoopBuilder::new().build();

    event_loop.set_activation_policy(ActivationPolicy::Accessory);
    let auto = auto_launch();

    let mut config = read_config();
    messages_thread();
    if config.listening_to_mail {
        mail_thread();
    }
    let (tx, rx) = mpsc::channel();
    update_thread(tx);

    let tray_menu_items = TrayMenuItems::build(&config);
    let tray_menu = TrayMenu::build(&tray_menu_items);
    let mut tray_icon = TrayIcon::build(tray_menu);
    tray_icon.as_mut().unwrap().set_icon_as_template(true);

    if config.hide_icon_forever {
        tray_icon
            .as_mut()
            .unwrap()
            .set_visible(false)
            .expect("set_visible failed");
    } else {
        tray_icon
            .as_mut()
            .unwrap()
            .set_visible(true)
            .expect("set_visible failed");
    }

    let menu_channel = MenuEvent::receiver();
    let tray_channel = TrayIconEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Ok(msg) = rx.try_recv() {
            if msg {
                let yes = MessageDialog::new()
                    .set_title(&t!("new-version"))
                    .set_text(&t!("new-version-text"))
                    .show_confirm()
                    .unwrap();
                if yes {
                    match replace_old_version() {
                        Ok(_) => {
                            info!("{}", t!("binary-file-replace-success"));
                            let reboot = MessageDialog::new()
                                .set_title(&t!("update-success"))
                                .set_text(&t!("update-success-text"))
                                .show_confirm()
                                .unwrap();
                            if reboot {
                                tray_icon.take();
                                *control_flow = ControlFlow::Exit;
                                Command::new("open")
                                    .arg(get_current_exe_path())
                                    .output()
                                    .expect("Failed to open MessAuto");
                            }
                        }
                        Err(e) => {
                            warn!("{}: {}", t!("binary-file-replace-failed"), e);
                            MessageDialog::new()
                                .set_title(&t!("update-failed"))
                                .set_text(&e.to_string())
                                .show_alert()
                                .unwrap();
                        }
                    }
                }
            }
        }
        if let Ok(event) = menu_channel.try_recv() {
            if event.id == tray_menu_items.quit_i.id() {
                tray_icon.take();
                *control_flow = ControlFlow::Exit;
            } else if event.id == tray_menu_items.check_hide_icon_for_now.id() {
                tray_icon
                    .as_mut()
                    .unwrap()
                    .set_visible(false)
                    .expect("set_visible failed");
            } else if event.id == tray_menu_items.check_hide_icon_forever.id() {
                config.hide_icon_forever = true;
                tray_icon
                    .as_mut()
                    .unwrap()
                    .set_visible(false)
                    .expect("set_visible failed");
                config.update().expect("failed to update config");
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
                        config.update().expect("failed to update config");
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
                    config.update().expect("failed to update config");
                }
            } else if event.id == tray_menu_items.check_auto_return.id() {
                config.auto_return = tray_menu_items.check_auto_return.is_checked();
                config.update().expect("failed to update config");
            } else if event.id == tray_menu_items.check_launch_at_login.id() {
                if tray_menu_items.check_launch_at_login.is_checked() {
                    auto.enable().expect("failed to enable auto launch");
                    if auto.is_enabled().unwrap() {
                        info!("{}", t!("set-launch-at-login"));
                        config.launch_at_login = true;
                        config.update().expect("failed to update config");
                    } else {
                        info!("{}", t!("disable-launch-at-login"));
                        tray_menu_items.check_launch_at_login.set_checked(false);
                    }
                } else {
                    auto.disable().expect("failed to disable auto launch");
                    if !auto.is_enabled().unwrap() {
                        config.launch_at_login = false;
                        config.update().expect("failed to update config");
                    } else {
                        tray_menu_items.check_launch_at_login.set_checked(true);
                    }
                }
                // } else if event.id == tray_menu_items.add_flag.id() {
                //     println!("add flag");
            } else if event.id == tray_menu_items.maconfig.id() {
                Command::new("open")
                    .arg(config_path())
                    .output()
                    .expect("Failed to open config");
            } else if event.id == tray_menu_items.logs.id() {
                Command::new("open")
                    .arg(log_path())
                    .output()
                    .expect("Failed to open logs");
            } else if event.id == tray_menu_items.listening_to_mail.id() {
                if tray_menu_items.listening_to_mail.is_checked() {
                    config.listening_to_mail = true;
                    config.update().expect("failed to update config");
                    mail_thread();
                    info!("{}", t!("mail-listening-enabled"));
                } else {
                    config.listening_to_mail = false;
                    config.update().expect("failed to update config");
                    info!("{}", t!("mail-listening-disabled"));
                }
            } else if event.id == tray_menu_items.float_window.id() {
                if tray_menu_items.float_window.is_checked() {
                    config.float_window = true;
                    config.update().expect("failed to update config");
                    info!("{}", t!("float-window-enabled"));
                } else {
                    config.float_window = false;
                    config.update().expect("failed to update config");
                    info!("{}", t!("float-window-disabled"));
                }
            } else {
                warn!("{}", t!("unknown-operation"));
            }
        }
        if let Ok(event) = tray_channel.try_recv() {
            trace!("{event:?}");
        }
    });
}
