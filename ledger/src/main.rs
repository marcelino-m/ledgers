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
    journal::{Journal, Xact},
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
            if let Err(msg) = args.period.validate() {
                eprintln!("error: {msg}");
                std::process::exit(2);
            }
            let price_db = open_price_db(&args.price_db_path);
            match util::read_journal_and_price_db(journal, price_db) {
                Ok((journal, price_db)) => {
                    let vtype = args.valuation.get();
                    let ledger = Ledger::from_xacts(filtered_xacts(&journal, &args.filter, &[]));

                    let bal = Balance::from_ledger(&ledger, &args.report_query);
                    let mut bal =
                        bal.to_balance_view_at_dates::<Holdings>(&price_db, args.period.at_dates());

                    if !args.display.empty {
                        bal.remove_zero_accounts();
                    };

                    if args.display.acc_depth > 0 {
                        bal = bal.limit_accounts_depth(args.display.acc_depth);
                    } else if args.display.collapse {
                        bal = bal.limit_accounts_depth(1)
                    };

                    let total_mode = match (args.display.no_total, args.display.only_total) {
                        (true, _) => printing::TotalMode::NoTotal,
                        (_, true) => printing::TotalMode::OnlyTotal,
                        _ => printing::TotalMode::Full,
                    };

                    let res = if args.display.flat {
                        printing::bal(
                            io::stdout(),
                            &bal.to_flat(),
                            total_mode,
                            args.annotate.map(|p| p.into()),
                            args.display.date_header,
                            vtype,
                            cli.fmt.into(),
                        )
                    } else {
                        printing::bal(
                            io::stdout(),
                            &bal.to_compact(),
                            total_mode,
                            args.annotate.map(|p| p.into()),
                            args.display.date_header,
                            vtype,
                            cli.fmt.into(),
                        )
                    };

                    if let Err(err) = res {
                        eprintln!("fail printing the report: {err}");
                        std::process::exit(1);
                    };

                    if args.warn_future && args.period.at.is_empty() {
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
                    let xacts = filtered_xacts(&journal, &args.filter, &args.report_query);
                    let reg = register::register(
                        xacts,
                        args.filter.end,
                        vtype,
                        args.display.acc_depth,
                        &price_db,
                    );

                    let reg = take_headtail(reg, args.display.head, args.display.tail);
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
        Commands::Print(args) => match util::read_journal_and_price_db(journal, None) {
            Ok((journal, _)) => {
                let it = filtered_xacts(&journal, &args.filter, &args.report_query);
                let it = take_headtail(it, args.display.head, args.display.tail);
                if let Err(err) = printing::prnt(io::stdout(), it, cli.fmt.into()) {
                    eprintln!("fail printing the report: {err}");
                    std::process::exit(1);
                };
            }
            Err(err) => {
                eprintln!("fail reading journal or price db: {err:?}");
                std::process::exit(1);
            }
        },
        Commands::Info(args) => match util::read_journal_and_price_db(journal, None) {
            Ok((journal, _price_db)) => {
                let xacts = filtered_xacts(&journal, &args.filter, &args.report_query);
                let report = info::scan(xacts);
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
        Commands::Schema(args) => {
            if let Err(msg) = printing::schema(io::stdout(), args.command) {
                eprintln!("{msg}");
                std::process::exit(2);
            }
        }
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
    #[arg(short = 'f', long = "file", global = true, help_heading = "Input")]
    journal_path: Option<String>,

    /// Format of report to generate.
    #[arg(long = "fmt", global = true, default_value_t = Fmt::Tty, value_enum, help_heading = "Display")]
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
    Info(InfoArgs),

    /// Print transactions matching the report-query in journal format.
    #[command(alias = "pr")]
    Print(PrintArgs),

    /// Print the JSON schema describing the `--fmt json` output of a
    /// command. Run with no argument to list the available schemas.
    Schema(SchemaArgs),
}

#[derive(Args)]
pub struct SchemaArgs {
    /// Report whose schema to print. Omit to list available schemas.
    pub command: Option<printing::Schema>,
}

#[derive(Args)]
pub struct InfoArgs {
    /// Restrict the report to transactions with at least one posting
    /// whose account name matches one of these regular expressions.
    /// Same syntax as in `balance`.
    pub report_query: Vec<Regex>,

    #[command(flatten)]
    filter: FilterFlags,
}

/// Report flags that pick the valuation method used to price holdings.
///
/// The four flags are mutually exclusive; passing more than one is an
/// error. Omitting all of them is equivalent to `--quantity`.
#[derive(Args)]
#[group(id = "valuation", required = false, multiple = false)]
struct ValuationFlags {
    /// Value each holding at its book value (acquisition cost in the
    /// settlement commodity).
    #[arg(short = 'B', long = "basis", alias = "cost", action = SetTrue, help_heading = "Valuation")]
    basis: Option<bool>,

    /// Report each holding at its most recent known price as of the
    /// reference date. Prices come from inline transaction prices
    /// (`@` / `@@`) and `P` directives; the most recent one wins.
    #[arg(short = 'V', long = "market", action = SetTrue,  help_heading = "Valuation")]
    market: Option<bool>,

    /// Value each holding at the market price that was in effect on the
    /// date of its acquisition (book value frozen at purchase time).
    #[arg(short = 'H', long = "historical", action = SetTrue, help_heading = "Valuation")]
    historical: Option<bool>,

    /// Report raw commodity amounts without any price conversion. This
    /// is the default when no other valuation flag is given.
    #[arg(short = 'O', long = "quantity", action = SetTrue, help_heading = "Valuation")]
    quantity: Option<bool>,
}

/// Flags that filter which transactions are considered in the report.
#[derive(Args)]
struct FilterFlags {
    /// Only transactions from that date forward will be considered.
    #[arg(short = 'b', long = "begin", help_heading = "Filter")]
    begin: Option<NaiveDate>,

    /// Transactions after that date will be discarded.
    #[arg(short = 'e', long = "end", help_heading = "Filter")]
    end: Option<NaiveDate>,

    /// Restrict the report to the transaction with this id. When set,
    /// the report query and date range are ignored.
    #[arg(long = "id", help_heading = "Filter")]
    id: Option<usize>,
}

/// Yields the transactions selected by the filter. When `--id` is set
/// it short-circuits to that single transaction; otherwise it applies
/// `--begin`/`--end` and the report query.
fn filtered_xacts<'a>(
    journal: &'a Journal,
    filter: &'a FilterFlags,
    query: &'a [Regex],
) -> Box<dyn Iterator<Item = &'a Xact> + 'a> {
    match filter.id {
        Some(target) => Box::new(journal.filter(move |x| x.id == target).take(1)),
        None => Box::new(journal.xact_filter_by(query, filter.begin, filter.end)),
    }
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

/// Balance flags that pick the reference dates and stepping interval.
#[derive(Args)]
#[clap(group(
    ArgGroup::new("period_group").args(["daily", "weekly", "monthly"])
))]
struct BalancePeriodFlags {
    /// Reference date(s) at which to evaluate the balance.
    ///
    /// Pass once to use as the base point for `--step` and a period flag
    /// (`--daily`/`--weekly`/`--monthly`). Pass multiple times
    /// (`--at 2026-01-01 --at 2026-02-01`) to evaluate the balance at
    /// exactly those dates, in the order given. Multi-`--at` is not
    /// compatible with `--step` or the period flags.
    ///
    /// Defaults to today if omitted.
    #[arg(long = "at", help_heading = "Period")]
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
        value_name = "[+/-]STEP",
        help_heading = "Period"
    )]
    step: i32,
}

/// Balance flags that shape how the report is rendered.
#[derive(Args)]
struct BalanceDisplayFlags {
    /// Show accounts whose total is zero.
    #[arg(short = 'E', long = "empty", help_heading = "Display")]
    empty: bool,

    /// Flatten the report instead of showing a hierarchical tree.
    #[arg(long = "flat", help_heading = "Display")]
    flat: bool,

    /// Display account names up to this depth only, 0 means unlimited.
    #[arg(long = "depth", default_value_t = 0, help_heading = "Display")]
    acc_depth: usize,

    /// Equivalent to `--depth=1`.
    #[arg(long, short = 'n', help_heading = "Display")]
    collapse: bool,

    /// Suppress the summary total shown at the bottom of the report.
    #[arg(
        long = "no-total",
        conflicts_with = "only_total",
        help_heading = "Display"
    )]
    no_total: bool,

    /// Show only the summary total, suppressing all account lines.
    #[arg(
        long = "only-total",
        conflicts_with = "no_total",
        help_heading = "Display"
    )]
    only_total: bool,

    /// Add a header line to the report showing date of the balance.
    #[arg(long = "date-header", help_heading = "Display")]
    date_header: bool,
}

#[derive(Args)]
pub struct BalanceArgs {
    /// One or more space-separated regular expressions. Only postings
    /// whose account name matches any of them are included. Patterns
    /// use Rust `regex` syntax, match anywhere in the name
    /// (case-sensitive; use `(?i)` for case-insensitive).
    report_query: Vec<Regex>,

    /// Path to the price database file.
    #[arg(long = "price-db", help_heading = "Input")]
    price_db_path: Option<String>,

    #[command(flatten)]
    filter: FilterFlags,

    #[command(flatten)]
    valuation: ValuationFlags,

    #[command(flatten)]
    period: BalancePeriodFlags,

    #[command(flatten)]
    display: BalanceDisplayFlags,

    /// Annotate amounts with price and gain (default `market`).
    #[arg(
        long = "annotate",
        global = true,
        value_enum,
        num_args = 0..=1,
        default_missing_value = "market",
        conflicts_with = "valuation",
    )]
    annotate: Option<Prices>,

    /// Warn if there are transactions dated after the `--at` date.
    #[arg(long = "warn-future", default_value_t = true, action = clap::ArgAction::Set)]
    warn_future: bool,
}

/// Print flags that shape how the report is rendered.
#[derive(Args)]
struct PrintDisplayFlags {
    /// Only show the top number transactions, can be combined with --tail.
    #[arg(long = "head", alias = "first", help_heading = "Display")]
    head: Option<usize>,

    /// Only show the bottom number transactions, can be combined with --head.
    #[arg(long = "tail", alias = "last", help_heading = "Display")]
    tail: Option<usize>,
}

#[derive(Args)]
pub struct PrintArgs {
    /// Restrict the report to transactions with at least one posting
    /// whose account name matches one of these regular expressions.
    /// Same syntax as in `balance`, but the entire transaction is
    /// included when one posting matches.
    pub report_query: Vec<Regex>,

    #[command(flatten)]
    filter: FilterFlags,

    #[command(flatten)]
    display: PrintDisplayFlags,
}

/// Register flags that shape how the report is rendered.
#[derive(Args)]
struct RegisterDisplayFlags {
    /// Only show the top number postings, can be combined with --tail.
    #[arg(long = "head", alias = "first", help_heading = "Display")]
    head: Option<usize>,

    /// Only show the bottom number postings can be combined with
    /// --head.
    #[arg(long = "tail", alias = "last", help_heading = "Display")]
    tail: Option<usize>,

    /// Display account names up to this depth only, 0 means unlimited.
    #[arg(long = "depth", default_value_t = 0, help_heading = "Display")]
    acc_depth: usize,
}

#[derive(Args)]
pub struct RegisterArgs {
    /// Restrict the report to transactions with at least one posting
    /// whose account name matches one of these regular expressions.
    /// Same syntax as in `balance`, but the entire transaction is
    /// included when one posting matches.
    pub report_query: Vec<Regex>,

    /// Path to the price database file.
    #[arg(long = "price-db", help_heading = "Input")]
    price_db_path: Option<String>,

    #[command(flatten)]
    filter: FilterFlags,

    #[command(flatten)]
    valuation: ValuationFlags,

    #[command(flatten)]
    display: RegisterDisplayFlags,
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

impl BalancePeriodFlags {
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
