//! Línea de comandos de rvscope.

use std::fs;
use std::process::ExitCode;

use rv_asm::{Program, assemble};
use rv_isa::{Group, INSTRUCTIONS, PSEUDO_INSTRUCTIONS, RegisterStyle};

const USAGE: &str = "uso: rvscope <orden>

órdenes:
  asm <fichero.s>   ensambla y muestra el listado (dirección, palabra, instrucción, línea)
  isa               lista las instrucciones y pseudoinstrucciones admitidas
  version           muestra la versión";

fn print_listing(program: &Program, source: &str) {
    let lines: Vec<&str> = source.lines().collect();
    for entry in &program.listing {
        let text = entry
            .instruction
            .disassemble(entry.address, RegisterStyle::Abi);
        let original = lines.get(entry.line - 1).map_or("", |line| line.trim());
        println!(
            "{:08x}  {:08x}  {text:<28} {:>4}: {original}",
            entry.address, entry.word, entry.line
        );
    }
    if !program.data.bytes.is_empty() {
        println!(
            "\n.data: {} bytes desde 0x{:08x}",
            program.data.bytes.len(),
            program.data.base
        );
    }
}

fn run_asm(path: &str) -> ExitCode {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("rvscope: no se pudo leer {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    match assemble(&source) {
        Ok(program) => {
            print_listing(&program, &source);
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in &errors {
                eprintln!("{path}:{}:{}: {}", error.line, error.column, error.message);
            }
            ExitCode::FAILURE
        }
    }
}

fn print_isa() {
    for group in Group::ALL {
        println!("{}", group.title());
        for spec in INSTRUCTIONS.iter().filter(|spec| spec.group == group) {
            println!("  {:<24} {}", spec.syntax(), spec.summary);
        }
    }
    println!("Pseudoinstrucciones");
    for pseudo in &PSEUDO_INSTRUCTIONS {
        println!("  {:<24} {}", pseudo.syntax, pseudo.summary);
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["version" | "--version"] => {
            println!("rvscope {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        ["asm", path] => run_asm(path),
        ["isa"] => {
            print_isa();
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
