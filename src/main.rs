use clipboard::ClipboardContext;
use clipboard::ClipboardProvider;
use enigo::KeyboardControllable;
use enigo::{Enigo, Key};
use home::home_dir;
use regex::Regex;
use std::fs;
use std::process::Command;
use std::str;
use std::time::Duration;
use tray_item::TrayItem;

fn main() {
    let mut tray = TrayItem::new("📨", "").unwrap();
    tray.add_menu_item("启动", || {
        std::thread::spawn(move || {
            let auto_input = true;
            let flags = ["验证码", "verification", "인증"]; // 验证码触发关键词，只有验证码中包含 flags 中的关键词才会触发后续动作
            let check_db_path = home_dir()
                .expect("获取用户目录失败")
                .join("Library/Messages/chat.db-wal");
            let mut last_metadata_modified = fs::metadata(&check_db_path)
                .expect("获取元数据失败")
                .modified()
                .unwrap();
            loop {
                let now_metadata = fs::metadata(&check_db_path)
                    .expect("获取元数据失败")
                    .modified()
                    .unwrap();
                if now_metadata != last_metadata_modified {
                    last_metadata_modified = now_metadata;
                    let stdout = get_message_in_one_minute();
                    let (captcha_or_other, keyword) = check_captcha_or_other(&stdout, &flags);
                    if captcha_or_other {
                        let captchas = get_captchas(&stdout);
                        println!("获取到的所有可能的验证码:{:?}", captchas);
                        let real_captcha = get_real_captcha(captchas, keyword, &stdout);
                        println!("选择出的真正验证码：{:?}", real_captcha);
                        let mut ctx: ClipboardContext = ClipboardProvider::new().unwrap();
                        ctx.set_contents(real_captcha.to_owned()).unwrap();
                        if auto_input {
                            input_and_enter();
                        }
                    }
                }
                std::thread::sleep(Duration::new(1, 0));
            }
        });
    })
    .unwrap();

    let inner = tray.inner_mut();
    inner.add_quit_item("退出");
    inner.display();
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
    let re = Regex::new(r"[a-zA-Z0-9]{4,6}").unwrap(); // 只提取4-6位数字与字母组合
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

// 如果信息中包含多个4-6位数字与字母组合（比如公司名称和验证码都是4-6位英文数字组合，例如CSDN）
// 则选取距离触发词最近的那个匹配到的字符串
fn get_real_captcha(captchas: Vec<String>, keyword: &str, stdout: &String) -> String {
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
}

// 模拟键盘操作：粘贴与回车
fn input_and_enter() {
    let mut enigo = Enigo::new();

    // Meta + v 粘贴
    enigo.key_down(Key::Meta);
    enigo.key_click(Key::Raw(0x09));
    enigo.key_up(Key::Meta);

    // 回车
    enigo.key_click(Key::Return);
}
