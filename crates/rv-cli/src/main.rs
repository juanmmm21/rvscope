//! Línea de comandos de rvscope.

mod run;

use std::fs;
use std::io;
use std::process::ExitCode;

use rv_asm::{Program, assemble};
use rv_isa::{Group, INSTRUCTIONS, PSEUDO_INSTRUCTIONS, RegisterStyle};

use run::{Model, RunOptions, run_program};

const USAGE: &str = "uso: rvscope <orden>

órdenes:
  asm <fichero.s>   ensambla y muestra el listado (dirección, palabra, instrucción, línea)
  run <fichero.s> [opciones]
                    ejecuta el programa; lo que lee sale del stdin y lo que imprime va al
                    stdout; el motivo de la parada, los registros y la traza van al stderr
      --cpu <modelo>    un-ciclo (por defecto), 5-etapas-sin-riesgos (segmentado sin
                        forwarding ni detección de riesgos: lee valores viejos),
                        5-etapas-burbujas (segmentado que espera con burbujas en ID) o
                        5-etapas-forwarding (adelanta resultados a EX; solo espera tras
                        una carga)
      --trace           muestra cada instrucción retirada y lo que cambia
      --max-cycles <n>  tope de ciclos (por defecto 10000000)
      --numeric         registros como x0..x31 en vez de sus nombres del ABI
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

/// Lee y ensambla `path`; si falla, ya ha contado por qué en el stderr.
fn load(path: &str) -> Option<(Program, String)> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("rvscope: no se pudo leer {path}: {error}");
            return None;
        }
    };
    match assemble(&source) {
        Ok(program) => Some((program, source)),
        Err(errors) => {
            for error in &errors {
                eprintln!("{path}:{}:{}: {}", error.line, error.column, error.message);
            }
            None
        }
    }
}

fn run_asm(path: &str) -> ExitCode {
    match load(path) {
        Some((program, source)) => {
            print_listing(&program, &source);
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}

/// Separa el fichero y las opciones de `run`, en cualquier orden.
fn parse_run_args<'a>(args: &[&'a str]) -> Result<(&'a str, RunOptions), String> {
    let mut options = RunOptions::default();
    let mut path: Option<&str> = None;
    let mut rest = args.iter();
    while let Some(&arg) = rest.next() {
        match arg {
            "--trace" => options.trace = true,
            "--numeric" => options.style = RegisterStyle::Numeric,
            "--cpu" => {
                let name = rest.next().ok_or("--cpu necesita el nombre de un modelo")?;
                options.model = Model::parse(name).ok_or_else(|| {
                    let names: Vec<&str> = Model::NAMES.iter().map(|(name, _)| *name).collect();
                    format!(
                        "--cpu: no hay un modelo «{name}» (hay {})",
                        names.join(", ")
                    )
                })?;
            }
            "--max-cycles" => {
                let value = rest
                    .next()
                    .ok_or("--max-cycles necesita un número de ciclos")?;
                options.max_cycles = value
                    .replace('_', "")
                    .parse()
                    .map_err(|_| format!("--max-cycles: «{value}» no es un número de ciclos"))?;
            }
            flag if flag.starts_with("--") => return Err(format!("opción desconocida: {flag}")),
            file if path.is_none() => path = Some(file),
            extra => return Err(format!("sobra «{extra}»: run ejecuta un solo fichero")),
        }
    }
    path.map(|path| (path, options))
        .ok_or_else(|| "falta el fichero que ejecutar".to_string())
}

fn run_file(args: &[&str]) -> ExitCode {
    let (path, options) = match parse_run_args(args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("rvscope run: {message}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let Some((program, _)) = load(path) else {
        return ExitCode::FAILURE;
    };
    let stdin = io::stdin();
    let result = run_program(
        &program,
        &options,
        &mut stdin.lock(),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    );
    match result {
        Ok((ending, _)) => ExitCode::from(ending.exit_code()),
        Err(error) => {
            eprintln!("rvscope: {path}: {error}");
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
        ["run", rest @ ..] => run_file(rest),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_options_in_any_order() {
        let (path, options) =
            parse_run_args(&["--trace", "prog.s", "--max-cycles", "1_000", "--numeric"]).unwrap();
        assert_eq!(path, "prog.s");
        assert!(options.trace);
        assert_eq!(options.max_cycles, 1000);
        assert_eq!(options.style, RegisterStyle::Numeric);
        assert_eq!(options.model, Model::SingleCycle);
        let (_, options) = parse_run_args(&["--cpu", "5-etapas-sin-riesgos", "prog.s"]).unwrap();
        assert_eq!(options.model, Model::FiveStageNoHazards);
        let (_, options) = parse_run_args(&["--cpu", "5-etapas-burbujas", "prog.s"]).unwrap();
        assert_eq!(options.model, Model::FiveStageStalls);
        let (_, options) = parse_run_args(&["--cpu", "5-etapas-forwarding", "prog.s"]).unwrap();
        assert_eq!(options.model, Model::FiveStageForwarding);
    }

    #[test]
    fn run_args_are_checked() {
        assert!(parse_run_args(&[]).is_err());
        assert!(parse_run_args(&["a.s", "b.s"]).is_err());
        assert!(parse_run_args(&["a.s", "--max-cycles"]).is_err());
        assert!(parse_run_args(&["a.s", "--max-cycles", "mucho"]).is_err());
        assert!(parse_run_args(&["a.s", "--rapido"]).is_err());
        assert!(parse_run_args(&["a.s", "--cpu"]).is_err());
        assert!(parse_run_args(&["a.s", "--cpu", "turbo"]).is_err());
        assert_eq!(parse_run_args(&["a.s"]).unwrap().1, RunOptions::default());
    }
}
