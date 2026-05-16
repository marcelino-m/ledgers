use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;

use crate::{
    account_view::AccountView,
    amount::Amount,
    balance::Balance,
    balance::Valuation,
    journal::{AccName, Xact},
    pricedb::PriceDB,
};

/// The portion of the register report produced by a single transaction.
///
/// Each `RegisterGroup` corresponds to a single `Xact` and carries the
/// rows that survive the account-name filter and the optional
/// depth-based aggregation. A group with no rows is dropped from the
/// final output.
#[derive(Debug, Serialize)]
pub struct RegisterGroup<'a> {
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
#[derive(Debug, Serialize)]
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

/// Returns an iterator over `RegisterGroup`s — one per transaction —
/// whose rows match at least one of the given regex queries against
/// account names.
pub fn register<'a>(
    xacts: impl Iterator<Item = &'a Xact>,
    _mode: Valuation,
    qry: &[Regex],
    price_db: &PriceDB,
    depth: usize,
) -> impl Iterator<Item = RegisterGroup<'a>> {
    xacts
        .scan(Amount::new(), move |accum, xact| {
            let entries_source = if depth == 0 {
                xact.postings
                    .iter()
                    .filter(|p| qry.is_empty() || qry.iter().any(|r| r.is_match(&p.acc_name)))
                    .map(|p| {
                        (
                            p.acc_name.clone(),
                            p.value(_mode, p.date, price_db).unwrap().to_amount(),
                        )
                    })
                    .collect::<Vec<_>>()
            } else {
                Balance::from_xact(xact)
                    .to_balance_view_as_of(xact.date.txdate, price_db)
                    .limit_accounts_depth(depth)
                    .to_flat()
                    .into_accounts()
                    .filter(|p| qry.is_empty() || qry.iter().any(|r| r.is_match(p.name())))
                    .map(|p| {
                        (
                            p.name().clone(),
                            p.balance().clone().into_iter().next().unwrap().1,
                        )
                    })
                    .collect::<Vec<_>>()
            };

            Some(RegisterGroup {
                date: &xact.date.txdate,
                payee: &xact.payee,
                rows: entries_source
                    .into_iter()
                    .map(|(name, total)| {
                        *accum += &total;
                        RegisterRow {
                            acc_name: name,
                            total,
                            running_total: accum.clone(),
                        }
                    })
                    .collect(),
            })
        })
        .filter(|r| !r.rows.is_empty())
}
