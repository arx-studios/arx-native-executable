use crate::ast::Program;
use crate::sema::SemaResult;
use anyhow::{anyhow, Context as _};
use clap::{Parser, Subcommand};
use inkwell::context::Context;
use inkwell::targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine};
use inkwell::OptimizationLevel;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// Process exit codes, per docs/ANX-Usage-Flow-v1.md: 0 success, 1
/// compile-time error, 2 runtime error.
const EXIT_COMPILE_ERROR: u8 = 1;
const EXIT_RUNTIME_ERROR: u8 = 2;

#[derive(Parser)]
#[command(name = "anx", version, about = "The ANX language: interpreter + LLVM-backed native compiler")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Lex, parse, and type-check a file without running it
    Check { file: PathBuf },
    /// Run a file through the tree-walking interpreter
    Run { file: PathBuf },
    /// Compile a file to a native executable
    Build {
        file: PathBuf,
        /// Output executable path
        #[arg(short, long)]
        output: PathBuf,
    },
}

pub fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Check { file } => match frontend(&file) {
            Some(_) => ExitCode::SUCCESS,
            None => ExitCode::from(EXIT_COMPILE_ERROR),
        },
        Cmd::Run { file } => run(&file),
        Cmd::Build { file, output } => build(&file, &output),
    }
}

/// Shared lex → parse → sema pipeline for all three subcommands. Reports
/// every error to stderr (with the file name and line prefixed) and returns
/// `None` on any compile-time failure.
fn frontend(file: &Path) -> Option<(Program, SemaResult)> {
    let display = file.display();
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {display}: {e}");
            return None;
        }
    };

    let tokens = match crate::lexer::lex(&source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {display}:{e}");
            return None;
        }
    };

    let program = match crate::parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {display}:{e}");
            return None;
        }
    };

    match crate::sema::analyze(&program) {
        Ok(sema_result) => Some((program, sema_result)),
        Err(errors) => {
            for e in errors {
                eprintln!("error: {display}:{e}");
            }
            None
        }
    }
}

fn run(file: &Path) -> ExitCode {
    let Some((program, _sema)) = frontend(file) else {
        return ExitCode::from(EXIT_COMPILE_ERROR);
    };
    match crate::interp::interpret(&program) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("runtime error: {e}");
            ExitCode::from(EXIT_RUNTIME_ERROR)
        }
    }
}

fn build(file: &Path, output: &Path) -> ExitCode {
    let Some((program, sema_result)) = frontend(file) else {
        return ExitCode::from(EXIT_COMPILE_ERROR);
    };
    match build_native(&program, &sema_result, file, output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // Everything past a clean frontend is an internal failure
            // (codegen bug, missing cc, unwritable output) — not a
            // user-program error, but still a failed compile.
            eprintln!("error: {e:#}");
            ExitCode::from(EXIT_COMPILE_ERROR)
        }
    }
}

/// The runtime shim travels inside the `anx` binary itself and is compiled
/// fresh at each `anx build`, so a built `anx` needs no companion files —
/// only a system C compiler.
const RUNTIME_C: &str = include_str!("codegen/runtime.c");

fn build_native(
    program: &Program,
    sema_result: &SemaResult,
    source_file: &Path,
    output: &Path,
) -> anyhow::Result<()> {
    let context = Context::create();
    let module_name = source_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("anx_module");
    let mut codegen = crate::codegen::Codegen::new(&context, module_name, sema_result);
    codegen.compile(program);
    codegen
        .verify()
        .map_err(|e| anyhow!("internal codegen error (please report):\n{e}"))?;

    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| anyhow!("failed to initialize native target: {e}"))?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple)
        .map_err(|e| anyhow!("failed to resolve target triple: {e}"))?;
    let target_machine = target
        .create_target_machine(
            &triple,
            &TargetMachine::get_host_cpu_name().to_string(),
            &TargetMachine::get_host_cpu_features().to_string(),
            OptimizationLevel::Default,
            RelocMode::Default,
            CodeModel::Default,
        )
        .ok_or_else(|| anyhow!("failed to create target machine"))?;
    codegen.module().set_triple(&triple);

    let workdir = std::env::temp_dir().join(format!("anx-build-{}", std::process::id()));
    std::fs::create_dir_all(&workdir).context("creating build temp dir")?;
    let object_path = workdir.join(format!("{module_name}.o"));
    let runtime_path = workdir.join("anx_runtime.c");

    target_machine
        .write_to_file(codegen.module(), FileType::Object, &object_path)
        .map_err(|e| anyhow!("failed to emit object file: {e}"))?;
    std::fs::write(&runtime_path, RUNTIME_C).context("writing runtime shim")?;

    // cc compiles the shim and links it with the object file in one step,
    // pulling in libc (malloc/calloc/printf) and the platform's C startup
    // code that calls our i32 main wrapper. -Wl,-w silences ld's macOS
    // deployment-version mismatch warning: LLVM's default triple stamps the
    // object with a newer version than cc's default, which is harmless but
    // would print noise on every single build.
    let status = Command::new("cc")
        .arg(&object_path)
        .arg(&runtime_path)
        .arg("-Wl,-w")
        .arg("-o")
        .arg(output)
        .status()
        .context("running the system C compiler `cc` (is Xcode CLT / a C toolchain installed?)")?;
    if !status.success() {
        return Err(anyhow!("linking failed (cc exited with {status})"));
    }

    let _ = std::fs::remove_dir_all(&workdir);
    Ok(())
}
