//! La orden `run` probada como la usa alguien desde la terminal: binario, stdin y código de
//! salida.

#![allow(clippy::unwrap_used)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

/// Escribe `source` en un fichero temporal propio del test y ejecuta `rvscope run` sobre él.
fn rvscope_run(name: &str, source: &str, stdin: &str, extra: &[&str]) -> Output {
    let path: PathBuf =
        std::env::temp_dir().join(format!("rvscope-cli-{}-{name}.s", std::process::id()));
    std::fs::write(&path, source).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_rvscope"))
        .arg("run")
        .arg(&path)
        .args(extra)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(&path).unwrap();
    output
}

const SUM: &str = "# suma los enteros leídos hasta un 0
        li s0, 0
loop:   li a7, 5
        ecall
        beqz a0, done
        add s0, s0, a0
        j loop
done:   mv a0, s0
        li a7, 1
        ecall
        li a0, 10
        li a7, 11
        ecall
        li a0, 0
        li a7, 10
        ecall
";

#[test]
fn program_output_goes_to_stdout_and_report_to_stderr() {
    let output = rvscope_run("sum", SUM, "4\n5\n-2\n0\n", &[]);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("parada en la línea 16"), "{report}");
    assert!(report.contains("  s0 0x00000007"), "{report}");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn exit_code_comes_from_the_program() {
    let output = rvscope_run("code", "li a0, 42\nli a7, 93\necall\n", "", &[]);
    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn assembly_errors_show_line_and_column() {
    let output = rvscope_run("bad", "nop\naddi a0, a9, 1\n", "", &[]);
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(report.contains("bad.s:2:"), "{report}");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn trace_and_cycle_limit() {
    let output = rvscope_run(
        "spin",
        "loop: j loop\n",
        "",
        &["--trace", "--max-cycles", "3"],
    );
    let report = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        report.lines().filter(|line| line.contains("jal")).count(),
        3,
        "{report}"
    );
    assert!(report.contains("sin terminar tras 3 ciclos"), "{report}");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn bad_options_print_usage() {
    let output = rvscope_run("usage", "nop\n", "", &["--rapido"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("opción desconocida"));
}
