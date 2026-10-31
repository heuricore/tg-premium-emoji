use clap::Parser;

fn main() -> std::process::ExitCode {
    tge::run(tge::cli::Cli::parse())
}
