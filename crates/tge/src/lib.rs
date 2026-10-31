pub mod cli;
pub mod error;
pub mod model;
pub mod output;
pub mod schema;
pub mod validate;

use std::process::ExitCode;

use cli::{Cli, Command};
use error::Error;
use output::Ctx;

pub fn run(cli: Cli) -> ExitCode {
    let ctx = Ctx { json: cli.json };
    match dispatch(&ctx, cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            if matches!(err, Error::Invalid(_)) && ctx.json {
                // the issues document was already printed
            } else if ctx.json {
                println!("{}", serde_json::json!({ "error": err.to_string() }));
            } else {
                eprintln!("error: {err}");
            }
            ExitCode::from(err.exit_code())
        }
    }
}

fn dispatch(ctx: &Ctx, command: Option<Command>) -> Result<(), Error> {
    match command {
        None => Err(Error::Usage("no command given, try --help".into())),
        Some(Command::Schema { out }) => {
            schema::write_all(&out)?;
            ctx.emit(
                &format!("schemas written to {}", out.display()),
                serde_json::json!({ "out": out }),
            );
            Ok(())
        }
        Some(Command::Validate { path }) => {
            let Some(path) = path else {
                return Err(Error::Usage(
                    "catalog discovery is not available yet, pass a catalog PATH".into(),
                ));
            };
            let issues = validate::validate_catalog_dir(&path);
            let errors = issues
                .iter()
                .filter(|i| i.severity == validate::Severity::Error)
                .count();
            let human = issues
                .iter()
                .map(|i| {
                    format!(
                        "{}: {}: {}",
                        i.severity.label(),
                        i.file.display(),
                        i.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            let human = if human.is_empty() {
                "ok".to_string()
            } else {
                human
            };
            ctx.emit(
                &human,
                serde_json::json!({ "issues": issues, "errors": errors }),
            );
            if errors > 0 {
                return Err(Error::Invalid(errors));
            }
            Ok(())
        }
    }
}
