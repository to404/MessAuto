use std::{
    error::Error,
    fs,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};
use std::io::Read;
use std::thread::sleep;

use auto_launch::AutoLaunch;
use clipboard::{ClipboardContext, ClipboardProvider};
use emlx::parse_emlx;
use enigo::{Enigo, Key, KeyboardControllable};
use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use home::home_dir;
use log::{error, info, warn};
use macos_accessibility_client::accessibility::application_is_trusted_with_prompt;
use mail_parser::MessageParser;
use native_dialog::{MessageDialog, MessageType};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex_lite::Regex;
use rust_i18n::t;
use serde::{Deserialize, Serialize};
use sys_locale;
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu},
    TrayIconBuilder,
};

rust_i18n::i18n!("locales");
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
pub struct MAConfig {
    pub auto_paste: bool,
    pub auto_return: bool,
    pub hide_icon_forever: bool,
    pub launch_at_login: bool,
    pub flags: Vec<String>,
    pub listening_to_mail: bool,
}

impl Default for MAConfig {
    fn default() -> Self {
        MAConfig {
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
            listening_to_mail: false,
        }
    }
}

impl MAConfig {
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

pub fn log_path() -> std::path::PathBuf {
    let mut log_path = home_dir().unwrap();
    log_path.push(".config");
    log_path.push("messauto");
    log_path.push("messauto.log");
    if !log_path.exists() {
        std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    }
    log_path
}

pub fn read_config() -> MAConfig {
    if !config_path().exists() {
        let config = MAConfig::default();
        let config_str = serde_json::to_string(&config).unwrap();
        std::fs::create_dir_all(config_path().parent().unwrap()).unwrap();
        std::fs::write(config_path(), config_str).unwrap();
    }
    let config_str = std::fs::read_to_string(config_path()).unwrap();
    let config = serde_json::from_str(&config_str);
    if config.is_err() {
        let config = MAConfig::default();
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
    pub maconfig: MenuItem,
    pub listening_to_mail: CheckMenuItem,
}

impl TrayMenuItems {
    pub fn build(config: &MAConfig) -> Self {
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
        let maconfig = MenuItem::new(t!("config"), true, None);
        let listening_to_mail = CheckMenuItem::new(
            t!("listening-to-mail"),
            true,
            config.listening_to_mail,
            None,
        );
        TrayMenuItems {
            quit_i,
            check_auto_paste,
            check_auto_return,
            check_hide_icon_for_now,
            check_hide_icon_forever,
            check_launch_at_login,
            add_flag,
            maconfig,
            listening_to_mail,
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
            &tray_menu_items.maconfig,
            &PredefinedMenuItem::separator(),
            &tray_menu_items.listening_to_mail,
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
            warn!("访问受阻：没有完全磁盘访问权限");
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
            warn!("已弹出窗口提醒用户授权，软件将关闭等待用户重启");
            panic!("exit without full disk access");
        }
        _ => {}
    }
}

pub fn check_accessibility() -> bool {
    application_is_trusted_with_prompt()
}

// 检查最新信息是否是验证码类型,并返回关键词来辅助定位验证码
pub fn check_captcha_or_other<'a>(stdout: &'a String, flags: &'a Vec<String>) -> bool {
    for flag in flags {
        if stdout.contains(flag) {
            return true;
        }
    }
    false
}

// 利用正则表达式从信息中提取验证码
pub fn get_captchas(stdout: &String) -> Vec<String> {
    let re = Regex::new(r"\b[a-zA-Z0-9]{4,7}\b").unwrap(); // 只提取4-7位数字与字母组合
    let stdout_str = stdout.as_str();
    let mut captcha_vec = Vec::new();
    for m in re.find_iter(stdout_str) {
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

pub fn messages_thread() {
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
                let captcha_or_other = check_captcha_or_other(&stdout, &flags);
                if captcha_or_other {
                    info!("检测到新的验证码类型信息：{:?}", stdout);
                    let captchas = get_captchas(&stdout);
                    info!("所有可能的验证码为:{:?}", captchas);
                    let real_captcha = get_real_captcha(&stdout);
                    info!("提取到真正的验证码:{:?}", real_captcha);
                    let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                    ctx.set_contents(real_captcha.to_owned()).unwrap();
                    let config = read_config();
                    if config.auto_paste {
                        paste(&mut enigo);
                        info!("粘贴验证码");
                        if config.auto_return {
                            enter(&mut enigo);
                            info!("执行回车");
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
            Command::new("curl")
                .arg(download_url)
                .arg("--max-time")
                .arg("10")
                .arg("-L")
                .arg("-f")
                .arg("-o")
                .arg("/tmp/MessAuto.zip")
                .output()?;
        }
        "aarch64" => {
            let download_url = format!(
                "https://github.com/LeeeSe/MessAuto/releases/download/{}/MessAuto_aarch64.zip",
                latest_version.unwrap()
            );
            Command::new("curl")
                .arg(download_url)
                .arg("--max-time")
                .arg("10")
                .arg("-L")
                .arg("-f")
                .arg("-o")
                .arg("/tmp/MessAuto.zip")
                .output()?;
        }
        _ => {
            error!("不支持的平台");
        }
    }
    if !Path::new("/tmp/MessAuto.zip").exists() {
        warn!("新版本下载失败");
        return Err("Download failed".into());
    } else {
        info!("新版本下载成功");
    }
    Ok(())
}

pub fn update_thread(tx: std::sync::mpsc::Sender<bool>) {
    std::thread::spawn(move || {
        if check_for_updates().is_ok() {
            if check_for_updates().unwrap() {
                info!("检测到新版本");
                if download_latest_release().is_ok() {
                    tx.send(true).unwrap();
                }
            } else {
                info!("当前已是最新版本");
            }
        } else {
            warn!("检查更新失败，请确保网络可以正常访问 Github 及其相关 API");
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


pub fn mail_thread() {
    std::thread::spawn(move || {
        let mail_path = home_dir().unwrap().join("Library/Mail");
        let path = String::from(mail_path.to_str().unwrap());

        futures::executor::block_on(async {
            if let Err(e) = async_watch(path).await {
                error!("error: {:?}", e)
            }
        });
    });
}

fn async_watcher() -> notify::Result<(RecommendedWatcher, Receiver<notify::Result<Event>>)> {
    let (mut tx, rx) = channel(1);

    let watcher = RecommendedWatcher::new(
        move |res| {
            futures::executor::block_on(async {
                tx.send(res).await.unwrap();
            })
        },
        Config::default(),
    )?;

    Ok((watcher, rx))
}

async fn async_watch<P: AsRef<Path>>(path: P) -> notify::Result<()> {
    let (mut watcher, mut rx) = async_watcher()?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;

    while let Some(res) = rx.next().await {
        match res {
            Ok(event) => {
                match event.kind {
                    notify::event::EventKind::Create(_) => {
                        for path in event.paths {
                            let path = path.to_string_lossy();
                            if path.contains(".emlx") && path.contains("INBOX.mbox") {
                                info!("收到新邮件: {:?}", path);
                                let path = path.replace(".tmp", "");
                                let content = read_emlx(&path);
                                info!("len: {}", content.len());
                                info!("邮件内容：{:?}", content);
                                if content.len() < 500 {
                                    let is_captcha = check_captcha_or_other(&content, &read_config().flags);
                                    if is_captcha {
                                        info!("检测到新的验证码类型邮件：{:?}", content);
                                        let captchas = get_captchas(&content);
                                        info!("所有可能的验证码为:{:?}", captchas);
                                        let real_captcha = get_real_captcha(&content);
                                        info!("提取到真正的验证码:{:?}", real_captcha);
                                        let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                                        ctx.set_contents(real_captcha.to_owned()).unwrap();
                                        let config = read_config();
                                        if config.auto_paste {
                                            let mut enigo = Enigo::new();
                                            paste(&mut enigo);
                                            info!("粘贴验证码");
                                            if config.auto_return {
                                                enter(&mut enigo);
                                                info!("执行回车");
                                            }
                                        }
                                    }
                                }
                                sleep(std::time::Duration::from_secs(2));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => error!("watch error: {:?}", e),
        }
    }
    Ok(())
}

fn read_emlx<'x>(path: &str) -> String {
    let mut file = std::fs::File::open(path).unwrap();
    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer).unwrap();

    let parsed = parse_emlx(&buffer).unwrap();

    let message = std::str::from_utf8(parsed.message).unwrap();
    let message = MessageParser::default().parse(message).unwrap();

    message.body_text(0).unwrap().to_owned().to_string()
}
