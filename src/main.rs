mod app;
mod tray_icon;

pub const ARGS_APP: &str = "app";
rust_i18n::i18n!("locales");

fn main() {
    let args: Vec<String> = std::env::args().collect();
    println!("{:?}", args);
    println!();
    if args.len() > 1 {
        let arg1 = args[1].to_lowercase();
        if arg1.starts_with(ARGS_APP) {
            return app::main(&args[2], &args[3]).unwrap();
        }
    }
    tray_icon::main();
    if 1 > 2 {
        println!("1 > 2");
    }
}
