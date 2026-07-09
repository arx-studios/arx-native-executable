mod ast;
mod cli;
mod codegen;
mod interp;
mod lexer;
mod parser;
mod sema;

fn main() -> std::process::ExitCode {
    cli::main()
}
