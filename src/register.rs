use std::fmt::Debug;

use chrono::NaiveDate;
use regex::Regex;

use crate::{
    commodity::{Amount, Quantity, Valuation},
    journal::{AccName, Xact},
    pricedb::PriceDB,
};

/// Represent a entry in the register report
#[derive(Debug)]
pub struct Register<'a> {
    pub date: &'a NaiveDate,
    pub payee: &'a str,
    pub entries: Vec<RegisterEntry<'a>>,
}

#[derive(Debug)]
pub struct RegisterEntry<'a> {
    pub acc_name: &'a AccName,
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
        .scan(Amount::default(), move |accum, xact| {
            Some(Register {
                date: &xact.date.txdate,
                payee: &xact.payee,
                entries: xact
                    .postings
                    .iter()
                    .filter(|p| qry.is_empty() || qry.iter().any(|r| r.is_match(&p.acc_name)))
                    .map(|p| {
                        let val = p.value(mode, price_db);
                        *accum += val;
                        RegisterEntry {
                            acc_name: &p.acc_name,
                            quantity: val,
                            running_total: accum.clone(),
                        }
                    })
                    .collect(),
            })
        })
        .filter(|r| !r.entries.is_empty())
}
