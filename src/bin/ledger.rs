use std::fs::File;
use std::io::{self, BufRead};

use chrono::NaiveDate;
use clap::{ArgAction::SetTrue, Args, Parser, Subcommand};
use regex::Regex;

use ledger::{
    balance, commodity::Valuation, journal, ledger::Ledger, pricedb, pricedb::PriceDB, printing,
    register,
};

fn main() {
    let cli = Cli::parse();

    let mode = cli.valuation.get();

    let file = match File::open(&cli.journal_path) {
        Ok(file) => file,
        Err(err) => {
            println!("fail open {}: {err}", cli.journal_path);
            return;
        }
    };

    let journal = match journal::read_journal(file) {
        Ok(journal) => journal,
        Err(err) => {
            println!("parsing {:?} {:?}", cli.journal_path, err);
            return;
        }
    };

    let ledger = Ledger::from_journal(&journal);
    let ledger = ledger.filter_by_date(cli.begin, cli.end);

    let mut price_db = PriceDB::from_journal(&journal);
    if let Some(path) = cli.price_db_path {
        if let Err(err) = upsert_from_price_db(&mut price_db, &path) {
            match err {
                ParseError::IOErr(e) => {
                    println!("fail reading price db {}: {:?}", path, e);
                }
            }
            return;
        }
    }

    match cli.command {
        Commands::Balance(args) => {
            let mut bal = balance::trial_balance(&ledger, mode, &args.report_query, &price_db);
            if !args.flat {
                bal = bal.to_hierarchical();
            };

            let res = printing::bal(io::stdout(), &bal, args.no_total, args.empty);
            if let Err(err) = res {
                println!("fail printing the report: {err}");
            };
        }
        Commands::Register(args) => {
            let xacts = args.maybe_head_tail_xacts(&journal);
            let reg = register::register(xacts, mode, &args.report_query, &price_db);
            if let Err(err) = printing::reg(io::stdout(), reg) {
                println!("fail printing the report: {err}");
            };
        }
    }
}

#[derive(Parser)]
#[command(
    author,
    about,
    long_about = None)] // Read from `Cargo.toml`
struct Cli {
    /// The ledger file
    #[arg(short = 'f', long = "file")]
    journal_path: String,
    /// Only transactions from that date forward will be considered.
    #[arg(short = 'b', long = "begin")]
    begin: Option<NaiveDate>,
    /// Transactions after that date  will be discarded.
    #[arg(short = 'e', long = "end")]
    end: Option<NaiveDate>,

    /// Path tho the price database file
    #[arg(long = "price-db", global = true)]
    price_db_path: Option<String>,

    /// Valuation method to use for the reports.
    #[command(flatten)]
    valuation: ValuationArgs,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Print a balance report showing totals for postings that match
    /// report-query
    #[command(alias = "bal")]
    Balance(BalanceArgs),

    /// List all postings matching the report-query
    #[command(alias = "reg")]
    Register(RegisterArgs),
}

#[derive(Args)]
#[group(required = false, multiple = false)]
struct ValuationArgs {
    /// Report in terms of cost basis, not register quantities or value
    #[arg(short = 'B', long = "basis",  action=SetTrue, global = true)]
    basis: Option<bool>,

    /// Report in terms of cost basis, not register quantities or
    /// value
    #[arg(short = 'V', long = "market",  action=SetTrue,  global = true)]
    market: Option<bool>,

    /// Value commodities at the time of their acquisition
    #[arg(short = 'H', long = "historical",  action=SetTrue,  global = true)]
    historical: Option<bool>,

    /// Report commodity totals (this is the default).
    #[arg(short = 'O', long = "quantity",  action=SetTrue,  global = true)]
    quantity: Option<bool>,
}

#[derive(Args)]
pub struct BalanceArgs {
    /// Only accounts that match one of these regular expressions will be
    /// included in the report.
    pub report_query: Vec<Regex>,

    /// Show accounts whose total is zero
    #[arg(short = 'E', long = "empty")]
    empty: bool,

    /// Flatten the report instead of showing a hierarchical tree
    #[arg(long = "flat")]
    flat: bool,

    /// Suppress the summary total shown at the bottom of the report
    #[arg(long = "no-total")]
    no_total: bool,
}

#[derive(Args)]
pub struct RegisterArgs {
    /// Only accounts that match one of these regular expressions will be
    /// included in the report.
    pub report_query: Vec<Regex>,

    /// Only show the top number postings, can be combined with --tail
    #[arg(long = "head", alias = "first")]
    head: Option<usize>,

    /// Only show the bottom number postings can be combined with
    /// --head
    #[arg(long = "tail", alias = "last")]
    tail: Option<usize>,
}

impl ValuationArgs {
    fn get(self) -> Valuation {
        match (self.basis, self.market, self.historical, self.quantity) {
            (Some(true), Some(false), Some(false), Some(false)) => Valuation::Basis,
            (Some(false), Some(true), Some(false), Some(false)) => Valuation::Market,
            (Some(false), Some(false), Some(true), Some(false)) => Valuation::Historical,
            (Some(false), Some(false), Some(false), Some(true)) => Valuation::Quantity,
            _ => Valuation::Quantity,
        }
    }
}

impl RegisterArgs {
    /// Returns an iterator over transactions according to the head
    /// and tail
    fn maybe_head_tail_xacts<'a>(
        &self,
        journal: &'a journal::Journal,
    ) -> Box<dyn Iterator<Item = &'a journal::Xact> + 'a> {
        match (self.head, self.tail) {
            (Some(nh), None) => Box::new(journal.xacts_head(nh)),
            (None, Some(nt)) => Box::new(journal.xacts_tail(nt)),
            (None, None) => Box::new(journal.xacts()),
            (Some(nh), Some(nt)) => {
                let head = journal.xacts_head(nh);
                let tail = journal.xacts_tail(nt);
                Box::new(head.chain(tail))
            }
        }
    }
}

#[derive(Debug)]
enum ParseError {
    IOErr(io::Error),
}

fn upsert_from_price_db(price_db: &mut PriceDB, path: &str) -> Result<(), ParseError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(err) => return Err(ParseError::IOErr(err)),
    };
    let bread = io::BufReader::new(file);
    for line in bread.lines() {
        let line = match line {
            Ok(line) => line,
            Err(err) => return Err(ParseError::IOErr(err)),
        };

        let price = match pricedb::parse_market_price_line(&line) {
            Ok(price) => price,
            Err(_) => {
                // TODO: handling this (printing, counting, etc?)
                continue;
            }
        };
        price_db.upsert_price(price.sym, price.date_time, price.price);
    }

    return Ok(());
}
