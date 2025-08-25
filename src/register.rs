use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;

use crate::{
    account::AccountName,
    commodity::{Amount, Quantity, Valuation},
    journal::Journal,
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
    journal: &'a Journal,
    mode: Valuation,
    qry: &[Regex],
    price_db: &PriceDB,
) -> impl Iterator<Item = Register<'a>> {
    journal
        .xacts()
        .flat_map(move |xact| {
            xact.postings.iter().map(move |p| {
                let value = match mode {
                    Valuation::Quantity => p.quantity,
                    Valuation::Basis => p.book_value(),
                    Valuation::Market => p.market_value(price_db),
                    Valuation::Historical => p.historical_value(price_db),
                };

                (&xact.date.txdate, &xact.payee, &p.account, value)
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
