use crate::ledger::Ledger;
use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand};
use regex::Regex;
use std::fs::File;
use std::io;
pub mod account;
pub mod balance;
pub mod commodity;
pub mod journal;
pub mod ledger;
pub mod macros;
pub mod parser;
pub mod prices;
pub mod register;
pub mod symbol;

use balance::Mode;

fn main() {
    let cli = Cli::parse();
    let file = match File::open(&cli.file) {
        Ok(file) => file,
        Err(err) => {
            println!("fail open {}: {err}", cli.file);
            return;
        }
    };

    let journal = match journal::read_journal(file) {
        Ok(journal) => journal,
        Err(err) => {
            println!("parsing {:?} {:?}", cli.file, err);
            return;
        }
    };

    let ledger = Ledger::from_xacts(&journal);
    let ledger = ledger.filter_by_date(cli.begin, cli.end);

    match cli.command {
        Some(Commands::Balance(args)) => {
            let mode = match args.basis {
                true => Mode::Basis,
                false => Mode::Quantity,
            };

            let bal = balance::trial_balance(&ledger, mode);

            let err = if args.flat {
                balance::print_balance(io::stdout(), &bal)
            } else {
                balance::print_balance(io::stdout(), &bal.balance_cumulative())
            };

            if let Err(err) = err {
                println!("fail printing the report: {err}");
            };
        }
        Some(Commands::Register(args)) => {
            let reg = register::register(&journal, &args.report_query);
            if let Err(err) = register::print_register(io::stdout(), reg) {
                println!("fail printing the report: {err}");
            };
        }
        None => {}
    }
}

#[derive(Parser)]
#[command(
    version,
    author,
    about, long_about = None)] // Read from `Cargo.toml`
struct Cli {
    /// The ledger file
    #[arg(short, long)]
    file: String,
    /// Only transactions from that date forward will be considered.
    #[arg(short = 'b', long = "begin")]
    begin: Option<NaiveDate>,
    /// Transactions after that date  will be discarded.
    #[arg(short = 'e', long = "end")]
    end: Option<NaiveDate>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Print a balance report showing totals for postings that match
    /// report-query
    Balance(BalanceArgs),
    Register(RegisterArgs),
}

#[derive(Args)]
pub struct BalanceArgs {
    /// Report in terms of cost basis, not register quantities or value
    #[arg(short = 'B', long = "basis")]
    basis: bool,

    /// Show accounts whose total is zero
    #[arg(short = 'E', long = "empty")]
    empty: bool,

    /// Flatten the report instead of showing a hierarchical tree
    #[arg(long = "flat")]
    flat: bool,
}

#[derive(Args)]
pub struct RegisterArgs {
    /// Only accounts that match one of these regular expressions will be
    /// included in the report.
    pub report_query: Vec<Regex>,
}
