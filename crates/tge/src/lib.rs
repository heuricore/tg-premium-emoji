pub mod cli;
pub mod error;
pub mod output;

use std::process::ExitCode;

use cli::{Cli, Command};
use error::Error;
use output::Ctx;

pub fn run(cli: Cli) -> ExitCode {
    let ctx = Ctx { json: cli.json };
    match dispatch(&ctx, cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if ctx.json {
                println!("{}", serde_json::json!({ "error": err.to_string() }));
            } else {
                eprintln!("error: {err}");
            }
            ExitCode::from(err.exit_code())
        }
    }
}

fn dispatch(_ctx: &Ctx, command: Option<Command>) -> Result<(), Error> {
    match command {
        None => Err(Error::Usage("no command given, try --help".into())),
        Some(never) => match never {},
    }
}
