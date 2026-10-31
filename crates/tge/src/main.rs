use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    match tge::cli::Cli::try_parse() {
        Ok(cli) => tge::run(cli),
        Err(err) if !err.use_stderr() => {
            // --help and --version: clap prints to stdout, exit 0
            let _ = err.print();
            ExitCode::SUCCESS
        }
        Err(err) => {
            if std::env::args_os().any(|arg| arg == "--json") {
                let message = err.to_string();
                println!("{}", serde_json::json!({ "error": message.trim() }));
            } else {
                let _ = err.print();
            }
            ExitCode::from(2)
        }
    }
}
