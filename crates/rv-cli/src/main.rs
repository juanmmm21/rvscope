//! Línea de comandos de rvscope.

use std::process::ExitCode;

const USAGE: &str = "uso: rvscope <orden>\n\nórdenes:\n  version   muestra la versión";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("version") | Some("--version") => {
            println!("rvscope {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
