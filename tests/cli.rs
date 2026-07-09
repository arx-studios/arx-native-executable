//! CLI-level smoke tests for the `anx check|run|build` subcommands
//! themselves (subcommand plumbing, exit codes) — per
//! docs/ANX-Implementation-Plan-v1.md Phase 6's exit gate. The full
//! 20-program dual-path benchmark suite with `.expected` fixtures is
//! Phase 7's job, not this file's.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn anx() -> Command {
    Command::cargo_bin("anx").unwrap()
}

fn write_temp(name: &str, contents: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("anx-cli-tests");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    fs::write(&path, contents).unwrap();
    path
}

#[test]
fn check_accepts_a_valid_program() {
    let file = write_temp("valid_check.nx", "void main() { print(1); }");
    anx().arg("check").arg(&file).assert().success();
}

#[test]
fn check_rejects_a_compile_time_error_with_exit_1() {
    let file = write_temp("bad_check.nx", "void main() { print(undeclared); }");
    anx()
        .arg("check")
        .arg(&file)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("undeclared identifier"));
}

#[test]
fn run_executes_a_valid_program_and_prints_its_output() {
    let file = write_temp("valid_run.nx", "void main() { print(2 + 2); }");
    anx().arg("run").arg(&file).assert().success().stdout("4\n");
}

#[test]
fn run_reports_compile_errors_with_exit_1_and_runs_nothing() {
    let file = write_temp("bad_run.nx", "void main() { int x = true; }");
    anx()
        .arg("run")
        .arg(&file)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("type mismatch"));
}

#[test]
fn run_reports_runtime_errors_with_exit_2() {
    let file = write_temp(
        "runtime_error.nx",
        "void main() { int[] a = [1]; print(a[5]); }",
    );
    anx()
        .arg("run")
        .arg(&file)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("out of bounds"));
}

#[test]
fn build_rejects_a_compile_time_error_with_exit_1() {
    let file = write_temp("bad_build.nx", "int main() { }");
    let out = std::env::temp_dir().join("anx-cli-tests/bad_build_out");
    anx()
        .arg("build")
        .arg(&file)
        .arg("-o")
        .arg(&out)
        .assert()
        .code(1);
    assert!(!out.exists(), "no binary should be produced on a compile error");
}

/// The Implementation Plan's literal Phase 6 exit gate: `anx build` on the
/// binary-search benchmark produces a standalone native binary that runs
/// and prints the right output.
#[test]
fn build_produces_a_standalone_binary_that_runs_correctly() {
    let source = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/benchmarks/01_binary_search.nx"
    );
    let out = std::env::temp_dir().join("anx-cli-tests/binary_search_bin");
    let _ = fs::remove_file(&out);

    anx().arg("build").arg(source).arg("-o").arg(&out).assert().success();
    assert!(out.exists(), "anx build should produce the output binary");

    // Run the produced binary directly — not through `anx` at all — to
    // prove it's a genuine standalone executable, per PRD Goal 3.
    let output = std::process::Command::new(&out)
        .output()
        .expect("compiled binary should execute standalone");
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "3\n-1\n");
}

/// The compiled path's runtime guards (added alongside the CLI so `anx
/// build` output matches the interpreter's error behavior exactly) must
/// also exit 2 with the same message shape as `anx run`.
#[test]
fn compiled_binary_reports_runtime_errors_with_exit_2() {
    let file = write_temp(
        "compiled_runtime_error.nx",
        "void main() { int[] a = [1]; print(a[5]); }",
    );
    let out = std::env::temp_dir().join("anx-cli-tests/oob_bin");
    anx().arg("build").arg(&file).arg("-o").arg(&out).assert().success();

    let output = std::process::Command::new(&out).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("out of bounds"));
}
