use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;

use crate::{
    account::AccountName,
    commodity::{Amount, Quantity, Valuation},
    journal::Xact,
    prices::PriceDB,
};

/// Represent a entry in the register report
#[derive(Debug)]
pub struct Register<'a> {
    pub date: &'a NaiveDate,
    pub payee: &'a str,
    pub account: &'a AccountName,
    pub quantity: Quantity,
    pub running_total: Amount,
}

/// Returns an iterator over `Register` entries filtered by account
/// names matching some of the given regex queries.
pub fn register<'a>(
    xacts: impl Iterator<Item = &'a Xact>,
    mode: Valuation,
    qry: &[Regex],
    price_db: &PriceDB,
) -> impl Iterator<Item = Register<'a>> {
    xacts
        .flat_map(move |xact| {
            xact.postings.iter().map(move |p| {
                (
                    &xact.date.txdate,
                    &xact.payee,
                    &p.account,
                    p.value(mode, price_db),
                )
            })
        })
        .filter(|(_, _, acc, _)| qry.is_empty() || qry.iter().any(|r| r.is_match(&acc)))
        .scan(Amount::default(), |accum, (date, payee, acc, value)| {
            *accum += value;
            Some(Register {
                date: &date,
                payee: &payee,
                account: &acc,
                quantity: value,
                running_total: accum.clone(),
            })
        })
}
