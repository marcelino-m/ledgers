use std::collections::VecDeque;
use std::io;

use chrono::NaiveDate;
use clap::{ArgAction::SetTrue, Args, Parser, Subcommand};

use regex::Regex;

use ledger::{
    balance::Balance, commodity::Valuation, ledger::Ledger, printing, register, register::Register,
    util,
};

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Balance(args) => {
            match util::read_journal_and_price_db(cli.journal_path, cli.price_db_path) {
                Ok((journal, price_db)) => {
                    let vtype = cli.valuation.get();
                    let ledger = Ledger::from_journal(&journal);
                    let ledger = ledger.filter_by_date(cli.begin, cli.end);

                    let bal = Balance::from_ledger(&ledger, &args.report_query);
                    let mut bal = bal.to_balance_view(vtype, &price_db);
                    if !args.empty {
                        bal.remove_empty_accounts();
                    };

                    if args.acc_depth > 0 {
                        bal = bal.limit_accounts_depth(args.acc_depth);
                    }

                    let res = if args.flat {
                        printing::bal(io::stdout(), &bal, args.no_total, cli.fmt.into())
                    } else {
                        printing::bal(
                            io::stdout(),
                            &bal.to_compact(),
                            args.no_total,
                            cli.fmt.into(),
                        )
                    };

                    if let Err(err) = res {
                        eprintln!("fail printing the report: {err}");
                        std::process::exit(1);
                    };
                }
                Err(err) => {
                    eprintln!("fail reading journal or price db: {err:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Register(args) => {
            match util::read_journal_and_price_db(cli.journal_path, cli.price_db_path) {
                Ok((journal, price_db)) => {
                    let vtype = cli.valuation.get();
                    let journal = journal.filter_by_date(cli.begin, cli.end);
                    let reg = register::register(
                        journal.xacts(),
                        vtype,
                        &args.report_query,
                        &price_db,
                        args.acc_depth,
                    );

                    let reg = args.maybe_head_tail_xacts(reg);
                    if let Err(err) = printing::reg(io::stdout(), reg, cli.fmt.into()) {
                        eprintln!("fail printing the report: {err}");
                        std::process::exit(1);
                    };
                }
                Err(err) => {
                    eprintln!("fail reading journal or price db: {err:?}");
                    std::process::exit(1);
                }
            }
        }
    };
}

/// Output format of the reports
#[derive(clap::ValueEnum, Clone, Debug)]
enum Fmt {
    Tty,
    Json,
    Lisp,
}

impl From<Fmt> for printing::Fmt {
    fn from(arg: Fmt) -> Self {
        match arg {
            Fmt::Json => printing::Fmt::Json,
            Fmt::Tty => printing::Fmt::Tty,
            Fmt::Lisp => printing::Fmt::Lisp,
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
    valuation: ValuationFlags,

    /// Format of report to generate
    #[arg(long = "fmt", global = true, default_value_t = Fmt::Tty, value_enum)]
    fmt: Fmt,

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
struct ValuationFlags {
    /// Report in terms of cost basis, not register quantities or value
    #[arg(short = 'B', long = "basis", alias="cost",  action=SetTrue, global = true)]
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

    /// Display account names up to this depth only, 0 means unlimited
    #[arg(long = "depth", default_value_t = 0)]
    acc_depth: usize,

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

    /// Display account names up to this depth only, 0 means unlimited
    #[arg(long = "depth", default_value_t = 0)]
    acc_depth: usize,
}

impl ValuationFlags {
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
        mut reg: impl Iterator<Item = Register<'a>> + 'a,
    ) -> Box<dyn Iterator<Item = Register<'a>> + 'a> {
        match (self.head, self.tail) {
            (None, None) => Box::new(reg),
            (Some(nh), None) => Box::new(reg.take(nh)),
            (None, Some(nt)) => {
                let tail = VecDeque::with_capacity(nt);
                let tail = reg.fold(tail, |mut acc, x| {
                    if acc.len() == nt {
                        acc.pop_front();
                    }
                    acc.push_back(x);
                    acc
                });

                Box::new(tail.into_iter())
            }
            (Some(nh), Some(nt)) => {
                let mut result = Vec::with_capacity(nh + nt);
                for _ in 0..nh {
                    if let Some(x) = reg.next() {
                        result.push(x);
                    } else {
                        break;
                    }
                }

                let tail = VecDeque::with_capacity(nt);
                let tail = reg.fold(tail, |mut acc, x| {
                    if acc.len() == nt {
                        acc.pop_front();
                    }
                    acc.push_back(x);
                    acc
                });

                result.extend(tail);
                Box::new(result.into_iter())
            }
        }
    }
}
