pub mod cli;
pub mod commands;
pub mod error;
pub mod model;
pub mod output;
pub mod paths;
pub mod schema;
pub mod search;
pub mod snippet;
pub mod store;
pub mod validate;

use std::process::ExitCode;

use cli::{Cli, Command};
use error::Error;
use output::Ctx;
use paths::Env;
use store::catalogs::Catalogs;

pub fn run(cli: Cli) -> ExitCode {
    let ctx = Ctx {
        json: cli.json,
        env: Env::from_cli(&cli),
    };
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
        Some(Command::Catalogs) => commands::catalogs::run(ctx),
        Some(Command::Doctor { prune }) => commands::doctor::run(ctx, prune),
        Some(Command::Find {
            query,
            pack,
            catalog,
            fits,
            repaint,
            format,
            limit,
        }) => commands::find::run(
            ctx,
            commands::find::Args {
                query,
                pack,
                catalog,
                fits,
                repaint,
                format,
                limit,
            },
        ),
        Some(Command::Snippet { pack, target, text }) => {
            commands::snippet::run(ctx, &pack, &target, &text)
        }
        Some(Command::Schema { out }) => {
            schema::write_all(&out)?;
            ctx.emit(
                &format!("schemas written to {}", out.display()),
                serde_json::json!({ "out": out }),
            );
            Ok(())
        }
        Some(Command::Validate { path }) => {
            let issues = match path {
                Some(path) => validate::validate_catalog_dir(&path),
                None => {
                    let (catalogs, mut issues) = Catalogs::discover(&ctx.env);
                    for catalog in &catalogs.catalogs {
                        issues.extend(validate::validate_catalog_dir(&catalog.dir));
                    }
                    issues
                }
            };
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
