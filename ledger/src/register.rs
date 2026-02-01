use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;
use serde::Serialize;

use crate::{
    amount::Amount,
    balance::Balance,
    balance::Valuation,
    balance_view::AccountView,
    journal::{AccName, Xact},
    misc::today,
    pricedb::PriceDB,
};

/// Represent a entry in the register report
#[derive(Debug, Serialize)]
pub struct Register<'a> {
    pub date: &'a NaiveDate,
    pub payee: &'a str,
    pub entries: Vec<RegisterEntry>,
}

#[derive(Debug, Serialize)]
pub struct RegisterEntry {
    pub acc_name: AccName,
    pub total: Amount,
    pub running_total: Amount,
}

/// Returns an iterator over `Register` entries filtered by account
/// names matching some of the given regex queries.
pub fn register<'a>(
    xacts: impl Iterator<Item = &'a Xact>,
    _mode: Valuation,
    qry: &[Regex],
    price_db: &PriceDB,
    depth: usize,
) -> impl Iterator<Item = Register<'a>> {
    xacts
        .scan(Amount::default(), move |accum, xact| {
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
                    .to_balance_view_as_of(today(), price_db)
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

            Some(Register {
                date: &xact.date.txdate,
                payee: &xact.payee,
                entries: entries_source
                    .into_iter()
                    .map(|(name, total)| {
                        *accum += &total;
                        RegisterEntry {
                            acc_name: name,
                            total,
                            running_total: accum.clone(),
                        }
                    })
                    .collect(),
            })
        })
        .filter(|r| !r.entries.is_empty())
}
