use std::fs::File;

pub mod commodity;
pub mod journal;
pub mod parser;
pub mod symbol;

use clap::Parser;

#[derive(Parser)]
#[command(
    version,
    author,
    about, long_about = None)] // Read from `Cargo.toml`
struct Cli {
    /// file with journal data
    #[arg(short, long)]
    file: String,
}

fn main() {
    let cli = Cli::parse();

    let mut file = match File::open(&cli.file) {
        Ok(file) => file,
        Err(err) => {
            println!("fail open {}: {err}", cli.file);
            return;
        }
    };

    if let Err(err) = journal::read_journal(&mut file) {
        println!("some error {err:?}");
    };
}
