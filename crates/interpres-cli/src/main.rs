use clap::Parser;

/// Interpres command-line translator.
#[derive(Parser)]
#[command(name = "interpres", version, about)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
}
