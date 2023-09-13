use auto_launch::AutoLaunch;
use clipboard::{ClipboardContext, ClipboardProvider};
use enigo::{Enigo, Key, KeyboardControllable};
use home::home_dir;
use macos_accessibility_client::accessibility::application_is_trusted_with_prompt;
use native_dialog::{MessageDialog, MessageType};
use regex_lite::Regex;
use rust_i18n::t;
rust_i18n::i18n!("locales");
use std::{
    error::Error,
    fmt::format,
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use sys_locale;

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
    pub flags: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            auto_paste: false,
            auto_return: false,
            hide_icon_forever: false,
            launch_at_login: false,
            flags: vec![
                "验证码".to_string(),
                "verification".to_string(),
                "code".to_string(),
                "인증".to_string(),
                "代码".to_string(),
            ],
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
    let config = serde_json::from_str(&config_str);
    if config.is_err() {
        let config = Config::default();
        let config_str = serde_json::to_string(&config).unwrap();
        std::fs::write(config_path(), config_str).unwrap();
        return config;
    } else {
        return config.unwrap();
    }
}

pub struct TrayMenuItems {
    pub quit_i: MenuItem,
    pub check_auto_paste: CheckMenuItem,
    pub check_auto_return: CheckMenuItem,
    pub check_hide_icon_for_now: MenuItem,
    pub check_hide_icon_forever: MenuItem,
    pub check_launch_at_login: CheckMenuItem,
    pub add_flag: MenuItem,
    pub config: MenuItem,
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
        let add_flag = MenuItem::new(t!("add-flag"), true, None);
        let config = MenuItem::new(t!("config"), true, None);
        TrayMenuItems {
            quit_i,
            check_auto_paste,
            check_auto_return,
            check_hide_icon_for_now,
            check_hide_icon_forever,
            check_launch_at_login,
            add_flag,
            config,
        }
    }
}

pub struct TrayMenu {}

impl TrayMenu {
    pub fn build(tray_menu_items: &TrayMenuItems) -> Menu {
        let tray_menu = Menu::new();
        let _ = tray_menu.append_items(&[
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
            )
            .expect("create submenu failed"),
            &tray_menu_items.check_launch_at_login,
            &PredefinedMenuItem::separator(),
            // &tray_menu_items.add_flag,
            &tray_menu_items.config,
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
pub fn check_captcha_or_other<'a>(stdout: &'a String, flags: &'a Vec<String>) -> (bool, &'a str) {
    for flag in flags {
        if stdout.contains(flag) {
            return (true, flag);
        }
    }
    (false, "")
}

// 利用正则表达式从信息中提取验证码
pub fn get_captchas(stdout: &String) -> Vec<String> {
    let re = Regex::new(r"\b[a-zA-Z0-9]{4,7}\b").unwrap(); // 只提取4-7位数字与字母组合
    let stdout_str = stdout.as_str();
    let mut captcha_vec = Vec::new();
    for m in re.find_iter(stdout_str) {
        println!("find captcha: {}", m.as_str());
        for i in m.as_str().chars() {
            if i.is_digit(10) {
                captcha_vec.push(m.as_str().to_string());
                break;
            }
        }
    }
    return captcha_vec;
}

// 如果检测到 chat.db 有变动，则提取最近一分钟内最新的一条信息
pub fn get_message_in_one_minute() -> String {
    let output = Command::new("sqlite3")
                                .arg(home_dir().expect("获取用户目录失败").join("Library/Messages/chat.db"))
                                .arg("SELECT text FROM message WHERE datetime(date/1000000000 + 978307200,\"unixepoch\",\"localtime\") > datetime(\"now\",\"localtime\",\"-60 second\") ORDER BY date DESC LIMIT 1;")
                                .output()
                                .expect("sqlite命令运行失败");
    let stdout = String::from_utf8(output.stdout).unwrap();
    return stdout;
}

// 如果信息中包含多个4-7位数字与字母组合（比如公司名称和验证码都是4-7位英文数字组合，例如CSDN）
// 则选取数字字符个数最多的的那个字串作为验证码
pub fn get_real_captcha(stdout: &String) -> String {
    let captchas = get_captchas(stdout);
    let mut real_captcha = String::new();
    let mut max_digit_count = 0;
    for captcha in captchas {
        let mut digit_count = 0;
        for i in captcha.chars() {
            if i.is_digit(10) {
                digit_count += 1;
            }
        }
        if digit_count > max_digit_count {
            max_digit_count = digit_count;
            real_captcha = captcha;
        }
    }
    real_captcha
}

// paste code
fn paste(enigo: &mut Enigo) {
    check_accessibility();
    // Meta + v 粘贴
    thread::sleep(Duration::from_millis(100));
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
    thread::sleep(Duration::from_millis(100));
    enigo.key_click(Key::Return);
    thread::sleep(Duration::from_millis(100));
}

pub fn auto_thread() {
    std::thread::spawn(move || {
        let mut enigo = Enigo::new();
        let flags = read_config().flags;
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
                    let real_captcha = get_real_captcha(&stdout);
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
    path
}

pub fn check_for_updates() -> Result<bool, Box<dyn Error>> {
    // 通过运行curl命令获取最新版本号
    let output = Command::new("curl")
        .arg("https://api.github.com/repos/LeeeSe/MessAuto/releases/latest")
        .arg("--max-time")
        .arg("10")
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // 解析json
    let v: serde_json::Value = serde_json::from_str(&stdout)?;
    let latest_version = v["tag_name"].as_str();
    if latest_version.is_none() {
        return Err("Tag_name not found".into());
    }
    // 获取当前二进制文件的版本号
    let current_version = env!("CARGO_PKG_VERSION");
    // 格式化两个版本号,将字符串中的非数字字符去掉,并转换为数字
    let latest_version = latest_version
        .unwrap()
        .chars()
        .filter(|c| c.is_digit(10))
        .collect::<String>();
    let current_version = current_version
        .chars()
        .filter(|c| c.is_digit(10))
        .collect::<String>();
    // 转换为数字
    let latest_version = latest_version.parse::<i32>()?;
    let current_version = current_version.parse::<i32>()?;
    // 如果最新版本号大于当前版本号,则提示更新
    if latest_version > current_version {
        return Ok(true);
    }
    Ok(false)
}

pub fn download_latest_release() -> Result<(), Box<dyn Error>> {
    // 通过运行curl命令获取最新版本号
    let output = Command::new("curl")
        .arg("https://api.github.com/repos/LeeeSe/MessAuto/releases/latest")
        .arg("--max-time")
        .arg("10")
        .output()?;
    let stdout = String::from_utf8(output.stdout)?;
    // 解析json
    let v: serde_json::Value = serde_json::from_str(&stdout)?;
    let latest_version = v["tag_name"].as_str();
    if latest_version.is_none() {
        return Err("Tag_name not found".into());
    }
    // 检查本机为arm还是x86
    let arch = std::env::consts::ARCH;
    // 根据本机架构选择下载链接
    match arch {
        "x86_64" => {
            let download_url = format!(
                "https://github.com/LeeeSe/MessAuto/releases/download/{}/MessAuto_x86_64.zip",
                latest_version.unwrap()
            );
            let output = Command::new("curl")
                .arg(download_url)
                .arg("--max-time")
                .arg("10")
                .arg("-L")
                .arg("-f")
                .arg("-o")
                .arg("/tmp/MessAuto.zip")
                .output()?;
            if !Path::new("/tmp/MessAuto.zip").exists() {
                return Err("Download failed".into());
            }
        }
        "aarch64" => {
            let download_url = format!(
                "https://github.com/LeeeSe/MessAuto/releases/download/{}/MessAuto_aarch64.zip",
                latest_version.unwrap()
            );
            let output = Command::new("curl")
                .arg(download_url)
                .arg("--max-time")
                .arg("10")
                .arg("-L")
                .arg("-f")
                .arg("-o")
                .arg("/tmp/MessAuto.zip")
                .output()?;
            // 如果 /tmp/MessAuto.zip 文件不存在,则下载失败
            if !Path::new("/tmp/MessAuto.zip").exists() {
                return Err("Download failed".into());
            }
        }
        _ => {
            println!("不支持的平台");
        }
    }
    Ok(())
}

pub fn update_thread(tx: std::sync::mpsc::Sender<bool>) {
    std::thread::spawn(move || {
        if check_for_updates().is_ok() {
            if check_for_updates().unwrap() {
                println!("检测到新版本");
                if download_latest_release().is_ok() {
                    println!("成功下载新版本");
                    tx.send(true).unwrap();
                } else {
                    println!("下载新版本失败，请确保网络可以正常访问 Github");
                }
            } else {
                println!("当前已是最新版本");
            }
        } else {
            println!("检查更新失败，请确保网络可以正常访问 Github");
        }
    });
}

// 将下载好的新版本替换老版本
pub fn replace_old_version() -> Result<(), Box<dyn Error>> {
    Command::new("unzip")
        .arg("-o")
        .arg("/tmp/MessAuto.zip")
        .arg("-d")
        .arg("/tmp/")
        .output()?;

    Command::new("rm").arg("/tmp/MessAuto.zip").output()?;

    Command::new("mv")
        .arg("/tmp/MessAuto.app")
        .arg(get_current_exe_path())
        .output()?;
    Ok(())
}
