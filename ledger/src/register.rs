use chrono::NaiveDate;
use regex::Regex;

use crate::{
    account_view::AccountView,
    amount::Amount,
    balance::{Balance, Valuation},
    holdings::Holdings,
    iter::WithNext,
    journal::{AccName, Xact},
    misc,
    ntypes::{Valuable, Zero},
    pricedb::PriceDB,
};

/// The portion of the register report produced by a single transaction.
///
/// Each `RegisterGroup` corresponds to a single `Xact` and carries the
/// rows that survive the account-name filter and the optional
/// depth-based aggregation. A group with no rows is dropped from the
/// final output.
pub struct RegisterGroup<'a> {
    /// Id of the transaction
    pub id: usize,
    /// Transaction date (`Xact::date::txdate`).
    pub date: &'a NaiveDate,
    /// Transaction payee.
    pub payee: &'a str,
    /// Rows emitted for this transaction, in display order. Each row
    /// is derived from one posting (no depth limit) or from a group
    /// of postings that share the same truncated account name (under
    /// `--depth`).
    pub rows: Vec<RegisterRow>,
}

/// A single row in the register report.
///
/// When no depth limit is in effect, each row corresponds to one
/// posting of the transaction. Under `--depth N`, postings whose
/// account names share the first `N` components are collapsed into a
/// single row whose `total` is their sum.
pub struct RegisterRow {
    /// Account name shown for this row, truncated to the requested
    /// depth when `--depth` is in effect.
    pub acc_name: AccName,
    /// Net amount of this row: a single posting's value, or the sum
    /// of postings collapsed under depth truncation.
    pub total: Amount,
    /// Cumulative sum of `total` across every row emitted so far,
    /// including rows from earlier transactions in the report.
    pub running_total: Amount,
}

/// Turns transactions into register rows. One `RegisterGroup` per
/// transaction, in order. Empty groups are dropped — don't print
/// noise.
///
/// The loop is straightforward: pull entries out of each xact, run
/// them through a running-total accumulator, emit rows. That's it.
///
/// Market valuation has one extra wart: between transactions, the
/// value of what you already hold drifts with the price. We can't
/// ignore that, so after each xact we synthesize a `<Revalued>` row
/// pinned to the next reference date — the next xact's date, or, for
/// the last xact, `at` if supplied, otherwise the greater of the
/// xact's date and today. No drift, no row.
///
/// Transaction selection is done upstream (see [`Journal::xact_filter_by`]):
/// only pass in the transactions you want reported. Within each
/// transaction, `query` controls which postings become rows — postings
/// whose account name matches none of the patterns are dropped. Date
/// filtering is entirely upstream; this function does not drop any
/// transaction based on date.
///
/// Parameters:
///
/// - `xacts`: the transactions to report.
/// - `query`: account-name patterns. A posting is included only when
///   at least one pattern matches its account name. An empty slice
///   includes all postings.
/// - `at`: reference date for the trailing revaluation after the last
///   xact. `None` means open-ended; the revaluation then falls back to
///   the greater of the last xact's date and today.
/// - `vtype`: how to value postings. The market case is the only one
///   that triggers the revaluation logic above.
/// - `depth`: `0` means one row per posting, no collapsing. Positive
///   values truncate account names and merge whatever shares the
///   prefix.
/// - `price_db`: where prices come from. Used for historical and
///   market valuation; ignored otherwise.
pub fn register<'a>(
    xacts: impl Iterator<Item = &'a Xact>,
    query: &'a [Regex],
    at: Option<NaiveDate>,
    vtype: Valuation,
    depth: usize,
    price_db: &PriceDB,
) -> impl Iterator<Item = RegisterGroup<'a>> {
    let mut accum = Accum::default();
    WithNext::new(xacts)
        .map(move |(xact, next)| {
            let mut rows = Vec::new();

            for (name, value, qty) in xact_entries(xact, query, vtype, price_db, depth) {
                rows.push(accum.record_entry(name, value, qty));
            }

            if matches!(vtype, Valuation::Market) {
                if let Some(d) = revaluation_anchor(xact, next, at) {
                    if let Some(row) = accum.record_revaluation(d, price_db) {
                        rows.push(row);
                    }
                }
            }

            RegisterGroup {
                id: xact.id,
                date: &xact.date.txdate,
                payee: &xact.payee,
                rows,
            }
        })
        .filter(|r| !r.rows.is_empty())
}

#[derive(Default)]
struct Accum {
    value: Amount,
    qty: Amount,
}

impl Accum {
    fn record_entry(&mut self, name: AccName, value: Amount, qty: Amount) -> RegisterRow {
        self.value += &value;
        self.qty += &qty;
        RegisterRow {
            acc_name: name,
            total: value,
            running_total: self.value.clone(),
        }
    }

    fn record_revaluation(&mut self, at: NaiveDate, price_db: &PriceDB) -> Option<RegisterRow> {
        let revalued = price_db.value_as_of(at, self.qty.clone()).unwrap();
        let diff = revalued - self.value.clone();

        if diff.is_zero() {
            return None;
        }

        self.value += &diff;
        Some(RegisterRow {
            acc_name: AccName::from("<Revalued>"),
            total: diff,
            running_total: self.value.clone(),
        })
    }
}

/// Extracts the raw entries that a single transaction contributes to
/// the register report, before any accumulation or revaluation.
///
/// Each entry is a triple `(account_name, value, quantity)` where
/// `value` is the amount under the requested `valuation` and
/// `quantity` is the underlying commodity amount (used later to
/// compute market-price revaluations between transactions).
///
/// Only postings (or depth-aggregated accounts) whose name matches at
/// least one pattern in `query` are returned. An empty `query` passes
/// all postings through.
///
/// The shape of the entries depends on `depth`:
///
/// - `depth == 0`: one entry per matching posting, preserving
///   per-posting granularity. The `value` is derived directly from the
///   posting's quantity, book value, or historical price according to
///   `valuation`.
/// - `depth > 0`: postings are collapsed by truncating account names
///   to the first `depth` components, going through a balance view
///   that aggregates holdings before valuation. The `query` filter is
///   applied after aggregation, on the truncated name.
fn xact_entries<'a>(
    xact: &'a Xact,
    query: &'a [Regex],
    valuation: Valuation,
    price_db: &'a PriceDB,
    depth: usize,
) -> Box<dyn Iterator<Item = (AccName, Amount, Amount)> + 'a> {
    if depth == 0 {
        Box::new(xact.postings.iter()
            .filter(move |p| query.is_empty() || query.iter().any(|r| r.is_match(&p.acc_name)))
            .map(move |p| {
            let value = match valuation {
                Valuation::Quantity => p.quantity.to_amount(),
                Valuation::Basis | Valuation::Market => p.book_value().to_amount(),
                Valuation::Historical => match p.lot_date {
                    Some(date) => price_db.value_as_of(date, p.quantity).unwrap(),
                    None => p.book_value().to_amount(),
                },
            };
            (p.acc_name.clone(), value, p.quantity.to_amount())
        }))
    } else {
        Box::new(
            Balance::from_xact(xact)
                .to_balance_view_as_of::<Holdings>(xact.date.txdate, price_db)
                .limit_accounts_depth(depth)
                .to_flat()
                .into_accounts()
                .filter(move |p| query.is_empty() || query.iter().any(|r| r.is_match(p.name())))
                .map(move |p| {
                    let (_, holding) = p.balance().clone().into_iter().next().unwrap();
                    (
                        p.name().clone(),
                        holding.valued_in(valuation),
                        holding.valued_in(Valuation::Quantity),
                    )
                }),
        )
    }
}

/// Picks the date used to revalue holdings after `xact`.
///
/// Uses `next`'s date for every transaction except the last. For the
/// last one, uses `at` if supplied, otherwise the greater of `xact`'s
/// date and today.
fn revaluation_anchor(
    xact: &Xact,
    next: Option<&Xact>,
    at: Option<NaiveDate>,
) -> Option<NaiveDate> {
    match next {
        Some(n) => Some(n.date.txdate),
        None => at.or_else(|| Some(misc::today()).filter(|&today| today > xact.date.txdate)),
    }
}
