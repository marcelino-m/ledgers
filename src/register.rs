use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;

use crate::{
    commodity::{Amount, Quantity},
    journal::AccountName,
    ledger::Ledger,
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
pub fn register<'a>(ledger: &'a Ledger, qry: &[Regex]) -> impl Iterator<Item = Register<'a>> {
    ledger
        .get_accounts()
        .filter(|acc| qry.is_empty() || qry.iter().any(|r| r.is_match(acc.name)))
        .flat_map(|acc| acc.get_entries())
        .scan(Amount::default(), |accum, entry| {
            *accum += entry.posting.quantity;
            Some(Register {
                date: &entry.xact.date.txdate,
                payee: &entry.xact.payee,
                account: &entry.posting.account,
                quantity: entry.posting.quantity,
                running_total: accum.clone(),
            })
        })
}
