use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;

use crate::{
    account::AccountName,
    commodity::{Amount, Quantity},
    journal::Journal,
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
pub fn register<'a>(journal: &'a Journal, qry: &[Regex]) -> impl Iterator<Item = Register<'a>> {
    journal
        .iter()
        .flat_map(|xact| {
            xact.postings
                .iter()
                .map(|p| (&xact.date.txdate, &xact.payee, p))
        })
        .filter(|(_, _, p)| qry.is_empty() || qry.iter().any(|r| r.is_match(&p.account)))
        .scan(Amount::default(), |accum, (date, payee, posting)| {
            *accum += posting.quantity;
            Some(Register {
                date: &date,
                payee: &payee,
                account: &posting.account,
                quantity: posting.quantity,
                running_total: accum.clone(),
            })
        })
}
