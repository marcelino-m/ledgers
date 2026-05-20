use std::fs::File;
use std::io::BufRead;
use std::io::{self, BufReader};

use chrono::NaiveDate;
use clap::{ArgAction::SetTrue, ArgGroup, Args, Parser, Subcommand, ValueEnum};

use regex::Regex;

use ledger::{
    balance::{Balance, Valuation},
    holdings::Holdings,
    info,
    iter::take_headtail,
    ledger::Ledger,
    misc::{self, Step},
    printing, register, util,
};

fn main() {
    let cli = Cli::parse();

    let journal: Box<dyn BufRead> = match cli.journal_path {
        Some(path) => {
            let file = File::open(&path).unwrap_or_else(|e| {
                eprintln!("Error opening file '{}': {}", path, e);
                std::process::exit(1);
            });
            Box::new(BufReader::new(file))
        }
        None => Box::new(BufReader::new(io::stdin())),
    };

    match cli.command {
        Commands::Balance(args) => {
            if let Err(msg) = args.validate() {
                eprintln!("error: {msg}");
                std::process::exit(2);
            }
            let price_db = open_price_db(&args.price_db_path);
            match util::read_journal_and_price_db(journal, price_db) {
                Ok((journal, price_db)) => {
                    let vtype = args.valuation.get();
                    let ledger = Ledger::from_journal(&journal);
                    let ledger = ledger.filter_by_date(cli.begin, cli.end);

                    let bal = Balance::from_ledger(&ledger, &args.report_query);
                    let mut bal =
                        bal.to_balance_view_at_dates::<Holdings>(&price_db, args.at_dates());

                    if !args.empty {
                        bal.remove_zero_accounts();
                    };

                    if args.acc_depth > 0 {
                        bal = bal.limit_accounts_depth(args.acc_depth);
                    } else if args.collapse {
                        bal = bal.limit_accounts_depth(1)
                    };

                    let total_mode = match (args.no_total, args.only_total) {
                        (true, _) => printing::TotalMode::NoTotal,
                        (_, true) => printing::TotalMode::OnlyTotal,
                        _ => printing::TotalMode::Full,
                    };

                    let res = if args.flat {
                        printing::bal(
                            io::stdout(),
                            &bal.to_flat(),
                            total_mode,
                            args.annotate.map(|p| p.into()),
                            args.date_header,
                            vtype,
                            cli.fmt.into(),
                        )
                    } else {
                        printing::bal(
                            io::stdout(),
                            &bal.to_compact(),
                            total_mode,
                            args.annotate.map(|p| p.into()),
                            args.date_header,
                            vtype,
                            cli.fmt.into(),
                        )
                    };

                    if let Err(err) = res {
                        eprintln!("fail printing the report: {err}");
                        std::process::exit(1);
                    };

                    if args.warn_future && args.at.is_empty() {
                        let today = misc::today();
                        let has_future = journal.xacts().any(|x| x.date.txdate > today);
                        if has_future {
                            eprintln!("warning: there are transactions dated after today");
                        }
                    }
                }
                Err(err) => {
                    eprintln!("fail reading journal or price db: {err:?}");
                    std::process::exit(1);
                }
            }
        }
        Commands::Register(args) => {
            let price_db = open_price_db(&args.price_db_path);
            match util::read_journal_and_price_db(journal, price_db) {
                Ok((journal, price_db)) => {
                    let vtype = args.valuation.get();
                    let reg = register::register(
                        journal.xacts(),
                        vtype,
                        args.acc_depth,
                        &price_db,
                        &args.report_query,
                        cli.begin,
                        cli.end,
                    );

                    let reg = take_headtail(reg, args.head, args.tail);
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
        Commands::Info => match util::read_journal_and_price_db(journal, None) {
            Ok((journal, _price_db)) => {
                let journal = journal.filter_by_date(cli.begin, cli.end);
                let report = info::scan(&journal);
                if let Err(err) = printing::info(io::stdout(), &report, cli.fmt.into()) {
                    eprintln!("fail printing the report: {err}");
                    std::process::exit(1);
                };
            }
            Err(err) => {
                eprintln!("fail reading journal or price db: {err:?}");
                std::process::exit(1);
            }
        },
    };
}

fn open_price_db(path: &Option<String>) -> Option<Box<dyn BufRead>> {
    path.as_ref().map(|path| -> Box<dyn BufRead> {
        let file = File::open(path).unwrap_or_else(|e| {
            eprintln!("Error opening price db file '{}': {}", path, e);
            std::process::exit(1);
        });
        Box::new(BufReader::new(file))
    })
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
    /// The ledger file.
    #[arg(short = 'f', long = "file")]
    journal_path: Option<String>,
    /// Only transactions from that date forward will be considered.
    #[arg(short = 'b', long = "begin")]
    begin: Option<NaiveDate>,
    /// Transactions after that date  will be discarded.
    #[arg(short = 'e', long = "end")]
    end: Option<NaiveDate>,

    /// Format of report to generate.
    #[arg(long = "fmt", global = true, default_value_t = Fmt::Tty, value_enum)]
    fmt: Fmt,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Print a balance report showing totals for postings that match
    /// report-query.
    #[command(alias = "bal")]
    Balance(BalanceArgs),

    /// List all postings matching the report-query.
    #[command(alias = "reg")]
    Register(RegisterArgs),

    /// List all accounts and commodities used in the journal.
    #[command(alias = "inf")]
    Info,
}

#[derive(Args)]
#[group(required = false, multiple = false)]
struct ValuationFlags {
    /// Report in terms of cost basis, not register quantities or value.
    #[arg(short = 'B', long = "basis", alias = "cost", action = SetTrue)]
    basis: Option<bool>,

    /// Report in terms of cost basis, not register quantities or
    /// value.
    #[arg(short = 'V', long = "market", action = SetTrue)]
    market: Option<bool>,

    /// Value commodities at the time of their acquisition.
    #[arg(short = 'H', long = "historical", action = SetTrue)]
    historical: Option<bool>,

    /// Report commodity totals (this is the default).
    #[arg(short = 'O', long = "quantity", action = SetTrue)]
    quantity: Option<bool>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Period {
    Daily,
    Weekly,
    Monthly,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Prices {
    #[value(alias = "B", alias = "b")]
    Basis,
    #[value(alias = "M", alias = "m", alias = "V", alias = "v")]
    Market,
    #[value(alias = "H", alias = "h")]
    Hist,
}

impl From<Prices> for Valuation {
    fn from(arg: Prices) -> Self {
        match arg {
            Prices::Basis => Valuation::Basis,
            Prices::Market => Valuation::Market,
            Prices::Hist => Valuation::Historical,
        }
    }
}

#[derive(Args)]
#[clap(group(
    ArgGroup::new("period_group")
        .args(["daily", "weekly", "monthly"])
))]
pub struct BalanceArgs {
    /// Only accounts that match one of these regular expressions will be
    /// included in the report.
    report_query: Vec<Regex>,

    /// Path to the price database file.
    #[arg(long = "price-db")]
    price_db_path: Option<String>,

    /// Valuation method to use for the report.
    #[command(flatten)]
    valuation: ValuationFlags,

    /// Show accounts whose total is zero.
    #[arg(short = 'E', long = "empty")]
    empty: bool,

    /// Flatten the report instead of showing a hierarchical tree.
    #[arg(long = "flat")]
    flat: bool,

    /// Display account names up to this depth only, 0 means unlimited.
    #[arg(long = "depth", default_value_t = 0)]
    acc_depth: usize,

    /// the same as --depth=1
    #[arg(long, short = 'n')]
    collapse: bool,

    /// Suppress the summary total shown at the bottom of the report.
    #[arg(long = "no-total", conflicts_with = "only_total")]
    no_total: bool,

    /// Show only the summary total, suppressing all account lines.
    #[arg(long = "only-total", conflicts_with = "no_total")]
    only_total: bool,

    /// Annotate each amount with its price and gain under the given valuation.
    #[arg(long = "annotate", global = true, value_enum)]
    annotate: Option<Prices>,

    /// Reference date(s) at which to evaluate the balance.
    ///
    /// Pass once to use as the base point for `--step` and a period flag
    /// (`--daily`/`--weekly`/`--monthly`). Pass multiple times
    /// (`--at 2026-01-01 --at 2026-02-01`) to evaluate the balance at
    /// exactly those dates, in the order given. Multi-`--at` is not
    /// compatible with `--step` or the period flags.
    ///
    /// Defaults to today if omitted.
    #[arg(long = "at")]
    at: Vec<NaiveDate>,

    /// Use daily intervals starting from the `--at` date.
    #[arg(short = 'D', long = "daily", help_heading = "Period")]
    daily: bool,

    /// Use weekly intervals starting from the `--at` date.
    #[arg(short = 'W', long = "weekly", help_heading = "Period")]
    weekly: bool,

    /// Use monthly intervals starting from the `--at` date.
    #[arg(short = 'M', long = "monthly", help_heading = "Period")]
    monthly: bool,

    /// Number of periods to apply relative to `at`.
    ///
    /// - `0` evaluates the balance only at `at`
    /// - Positive values move forward in time
    /// - Negative values move backward in time
    #[arg(
        short = 's',
        long = "step",
        default_value_t = 0,
        value_name = "[+/-]STEP"
    )]
    step: i32,

    /// Add a header line to the report showing date of the balance.
    #[arg(long = "date-header")]
    date_header: bool,

    /// Warn if there are transactions dated after the `--at` date.
    #[arg(long = "warn-future", default_value_t = true, action = clap::ArgAction::Set)]
    warn_future: bool,
}

#[derive(Args)]
pub struct RegisterArgs {
    /// Only accounts that match one of these regular expressions will be
    /// included in the report.
    pub report_query: Vec<Regex>,

    /// Path to the price database file.
    #[arg(long = "price-db")]
    price_db_path: Option<String>,

    /// Valuation method to use for the report.
    #[command(flatten)]
    valuation: ValuationFlags,

    /// Only show the top number postings, can be combined with --tail.
    #[arg(long = "head", alias = "first")]
    head: Option<usize>,

    /// Only show the bottom number postings can be combined with
    /// --head.
    #[arg(long = "tail", alias = "last")]
    tail: Option<usize>,

    /// Display account names up to this depth only, 0 means unlimited.
    #[arg(long = "depth", default_value_t = 0)]
    acc_depth: usize,
}

impl ValuationFlags {
    fn get(&self) -> Valuation {
        match (self.basis, self.market, self.historical, self.quantity) {
            (Some(true), Some(false), Some(false), Some(false)) => Valuation::Basis,
            (Some(false), Some(true), Some(false), Some(false)) => Valuation::Market,
            (Some(false), Some(false), Some(true), Some(false)) => Valuation::Historical,
            (Some(false), Some(false), Some(false), Some(true)) => Valuation::Quantity,
            _ => Valuation::Quantity,
        }
    }
}

impl BalanceArgs {
    pub fn get_period(&self) -> Period {
        if self.daily {
            Period::Daily
        } else if self.weekly {
            Period::Weekly
        } else {
            Period::Monthly
        }
    }

    pub fn at_dates(&self) -> Box<dyn Iterator<Item = NaiveDate>> {
        if self.at.len() > 1 {
            return Box::new(self.at.clone().into_iter());
        }
        let base = self.at.first().copied().unwrap_or_else(misc::today);
        let step = match self.get_period() {
            Period::Daily => Step::Days(self.step),
            Period::Weekly => Step::Weeks(self.step),
            Period::Monthly => Step::Months(self.step),
        };
        Box::new(misc::iter_dates(base, step))
    }

    fn validate(&self) -> Result<(), &'static str> {
        if self.at.len() > 1 && (self.daily || self.weekly || self.monthly || self.step != 0) {
            return Err(
                "multiple --at values cannot be combined with --step, --daily, --weekly or --monthly",
            );
        }
        Ok(())
    }
}
