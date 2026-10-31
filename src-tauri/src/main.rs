#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--self-check") {
        let Some(dir) = args.get(i + 1) else {
            eprintln!("usage: landau-bridge --self-check <dir>");
            std::process::exit(2);
        };
        match landau_bridge_lib::self_check(std::path::Path::new(dir)) {
            Ok(v) => {
                println!("self-check ok: schema {v}");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("self-check failed: {e}");
                std::process::exit(1);
            }
        }
    }
    landau_bridge_lib::run()
}
