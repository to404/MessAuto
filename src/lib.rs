use auto_launch::AutoLaunch;
use clipboard::{ClipboardContext, ClipboardProvider};
use enigo::{Enigo, Key, KeyboardControllable};
use home::home_dir;
use macos_accessibility_client::accessibility::application_is_trusted_with_prompt;
use native_dialog::{MessageDialog, MessageType};
use regex::Regex;
use rust_i18n::t;
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use sys_locale;
rust_i18n::i18n!("locales");
use serde::{Deserialize, Serialize};
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    TrayIconBuilder,
};

pub fn get_sys_locale() -> &'static str {
    let syslocal = sys_locale::get_locale().unwrap();
    // 只取前两个字符并转换为&str
    let lang_code = &syslocal[0..2];
    match lang_code {
        "zh" => "zh-CN",
        "en" => "en",
        _ => "en",
    }
}

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub auto_paste: bool,
    pub auto_return: bool,
    pub hide_icon_forever: bool,
    pub launch_at_login: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            auto_paste: false,
            auto_return: false,
            hide_icon_forever: false,
            launch_at_login: false,
        }
    }
}

impl Config {
    // update the local config "~/.config/messauto/messauto.json"
    pub fn update(&self) -> Result<(), Box<dyn std::error::Error>> {
        let updated_config_str = serde_json::to_string(&self)?;
        std::fs::write(config_path(), updated_config_str)?;
        Ok(())
    }
}

pub fn config_path() -> std::path::PathBuf {
    let mut config_path = home_dir().unwrap();
    config_path.push(".config");
    config_path.push("messauto");
    config_path.push("messauto.json");
    config_path
}

pub fn read_config() -> Config {
    if !config_path().exists() {
        let config = Config::default();
        let config_str = serde_json::to_string(&config).unwrap();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), config_str).unwrap();
    }
    let config_str = std::fs::read_to_string(config_path()).unwrap();
    let config: Config = serde_json::from_str(&config_str).unwrap();
    config
}

pub struct TrayMenuItems {
    pub quit_i: MenuItem,
    pub check_auto_paste: CheckMenuItem,
    pub check_auto_return: CheckMenuItem,
    pub check_hide_icon_for_now: MenuItem,
    pub check_hide_icon_forever: MenuItem,
    pub check_launch_at_login: CheckMenuItem,
}

impl TrayMenuItems {
    pub fn build(config: &Config) -> Self {
        let quit_i = MenuItem::new(t!("quit"), true, None);
        let check_auto_paste = CheckMenuItem::new(t!("auto-paste"), true, config.auto_paste, None);
        let check_auto_return = CheckMenuItem::new(
            t!("auto-return"),
            config.auto_paste,
            config.auto_return,
            None,
        );
        let check_hide_icon_for_now = MenuItem::new(t!("hide-icon-for-now"), true, None);
        let check_hide_icon_forever = MenuItem::new(t!("hide-icon-forever"), true, None);
        let check_launch_at_login =
            CheckMenuItem::new(t!("launch-at-login"), true, config.launch_at_login, None);
        TrayMenuItems {
            quit_i,
            check_auto_paste,
            check_auto_return,
            check_hide_icon_for_now,
            check_hide_icon_forever,
            check_launch_at_login,
        }
    }
}

pub struct TrayMenu {}

impl TrayMenu {
    pub fn build(tray_menu_items: &TrayMenuItems) -> Menu {
        let tray_menu = Menu::new();
        tray_menu.append_items(&[
            &tray_menu_items.check_auto_paste,
            &tray_menu_items.check_auto_return,
            &PredefinedMenuItem::separator(),
            &Submenu::with_items(
                t!("hide-icon"),
                true,
                &[
                    &tray_menu_items.check_hide_icon_for_now,
                    &tray_menu_items.check_hide_icon_forever,
                ],
            ),
            &tray_menu_items.check_launch_at_login,
            &PredefinedMenuItem::separator(),
            &tray_menu_items.quit_i,
        ]);
        tray_menu
    }
}

pub struct TrayIcon {}

impl TrayIcon {
    pub fn build(tray_menu: Menu) -> Option<tray_icon::TrayIcon> {
        Some(
            TrayIconBuilder::new()
                .with_menu(Box::new(tray_menu))
                .with_title("📨")
                .build()
                .unwrap(),
        )
    }
}

pub fn auto_launch() -> AutoLaunch {
    let app_name = env!("CARGO_PKG_NAME");
    let app_path = get_current_exe_path();
    // let app_path = std::path::Path::new("/Applications").join(format!("{}.app", app_name));
    println!("app_name: {:?}", app_name);
    println!("app_path: {:?}", app_path);
    let args = &["--minimized", "--hidden"];
    AutoLaunch::new(app_name, app_path.to_str().unwrap(), false, args)
}

pub fn check_full_disk_access() {
    // 试图访问敏感文件来触发权限请求
    let check_db_path = home_dir()
        .expect("获取用户目录失败")
        .join("Library/Messages");
    let ct = std::fs::read_dir(check_db_path);
    match ct {
        Err(_) => {
            let yes = MessageDialog::new()
                .set_type(MessageType::Info)
                .set_title(t!("full-disk-access").as_str())
                .show_confirm()
                .unwrap();
            if yes {
                Command::new("open")
                    .arg("/System/Library/PreferencePanes/Security.prefPane/")
                    .output()
                    .expect("Failed to open Disk Access Preferences window");
            }
            panic!("exit without full disk access");
        }
        _ => {}
    }
}

pub fn check_accessibility() -> bool {
    application_is_trusted_with_prompt()
}

// 检查最新信息是否是验证码类型,并返回关键词来辅助定位验证码
fn check_captcha_or_other<'a>(stdout: &'a String, flags: &'a [&'a str]) -> (bool, &'a str) {
    for flag in flags {
        if stdout.contains(flag) {
            return (true, flag);
        }
    }
    (false, "")
}

// 利用正则表达式从信息中提取验证码
fn get_captchas(stdout: &String) -> Vec<String> {
    let re = Regex::new(r"[a-zA-Z0-9]{4,7}").unwrap(); // 只提取4-7位数字与字母组合
    let stdout_str = stdout.as_str();
    let mut captcha_vec = Vec::new();
    for m in re.find_iter(stdout_str) {
        captcha_vec.push(m.as_str().to_string());
    }
    return captcha_vec;
}

// 如果检测到 chat.db 有变动，则提取最近一分钟内最新的一条信息
fn get_message_in_one_minute() -> String {
    let output = Command::new("sqlite3")
                                .arg(home_dir().expect("获取用户目录失败").join("Library/Messages/chat.db"))
                                .arg("SELECT text FROM message WHERE datetime(date/1000000000 + 978307200,\"unixepoch\",\"localtime\") > datetime(\"now\",\"localtime\",\"-60 second\") ORDER BY date DESC LIMIT 1;")
                                .output()
                                .expect("sqlite命令运行失败");
    let stdout = String::from_utf8(output.stdout).unwrap();
    return stdout;
}

// 如果信息中包含多个4-7位数字与字母组合（比如公司名称和验证码都是4-7位英文数字组合，例如CSDN）
// 则选取距离触发词最近的那个匹配到的字符串
fn get_real_captcha(captchas: Vec<String>, keyword: &str, stdout: &String) -> String {
    let result = find_string_with_most_digits(&captchas);
    if result.chars().filter(|c| c.is_digit(10)).count() == 0 {
        let keyword_location = stdout.find(keyword).unwrap() as i32;
        let mut min_distance = stdout.len() as i32;
        let mut real_captcha = String::new();
        for captcha in captchas {
            let captcha_location = stdout.find(&captcha).unwrap();
            let distance = (captcha_location as i32 - keyword_location as i32).abs();
            if distance < min_distance {
                min_distance = distance;
                real_captcha = captcha;
            }
        }
        return real_captcha;
    } else {
        result
    }
}

pub fn find_string_with_most_digits(v: &Vec<String>) -> String {
    let mut max_digits = 0;
    let mut result = String::new();

    for s in v {
        let digits = s.chars().filter(|c| c.is_digit(10)).count();
        if digits > max_digits {
            max_digits = digits;
            result = s.clone();
        }
    }

    result
}

// paste code
fn paste(enigo: &mut Enigo) {
    check_accessibility();
    // Meta + v 粘贴
    enigo.key_down(Key::Meta);
    thread::sleep(Duration::from_millis(100));
    enigo.key_click(Key::Raw(0x09));
    thread::sleep(Duration::from_millis(100));
    enigo.key_up(Key::Meta);
    thread::sleep(Duration::from_millis(100));
}

// enter the pasted code
fn enter(enigo: &mut Enigo) {
    check_accessibility();
    enigo.key_click(Key::Return);
    thread::sleep(Duration::from_millis(100));
}

pub fn auto_thread() {
    std::thread::spawn(move || {
        let mut enigo = Enigo::new();
        let flags = ["验证码", "verification", "code", "인증"]; // Captcha trigger keywords, only the keywords in flags in the captcha will trigger subsequent actions
        let check_db_path = home_dir().unwrap().join("Library/Messages/chat.db-wal");
        let mut last_metadata_modified = fs::metadata(&check_db_path).unwrap().modified().unwrap();
        loop {
            let now_metadata = fs::metadata(&check_db_path).unwrap().modified().unwrap();
            if now_metadata != last_metadata_modified {
                last_metadata_modified = now_metadata;
                let stdout = get_message_in_one_minute();
                let (captcha_or_other, keyword) = check_captcha_or_other(&stdout, &flags);
                if captcha_or_other {
                    let captchas = get_captchas(&stdout);
                    println!("All possible verification codes obtained:{:?}", captchas);
                    let real_captcha = get_real_captcha(captchas, keyword, &stdout);
                    println!("Select out the real verification code：{:?}", real_captcha);
                    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                    ctx.set_contents(real_captcha.to_owned()).unwrap();
                    let config = read_config();
                    if config.auto_paste {
                        paste(&mut enigo);
                        if config.auto_return {
                            enter(&mut enigo);
                        }
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(1));
        }
    });
}

pub fn get_current_exe_path() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    if path.to_str().unwrap().contains(".app") {
        path = path
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
    }
    // MessageDialog::new()
    //     .set_type(MessageType::Info)
    //     .set_title(path.to_str().unwrap())
    //     .show_confirm()
    //     .unwrap();
    path
}
