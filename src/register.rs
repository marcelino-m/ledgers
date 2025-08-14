use std::{
    fmt::Debug,
    io::{self, Write},
};

use chrono::NaiveDate;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Table};
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

pub fn print_register<'a>(
    mut out: impl Write,
    reg: impl Iterator<Item = Register<'a>>,
) -> io::Result<()> {
    let mut table = Table::new();
    table.load_preset(presets::NOTHING).set_header(
        ["Date", "Payee", "Account", "Amount", "RunningTotal"].map(|s| {
            Cell::new(s)
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Center)
        }),
    );

    table.add_rows(reg.into_iter().map(|r| {
        let running_total_str = r
            .running_total
            .iter()
            .map(|(k, v)| format!("{} {:.2}", k, v))
            .collect::<Vec<_>>()
            .join("\n");

        vec![
            Cell::new(r.date.to_string()),
            Cell::new(r.payee),
            Cell::new(r.account),
            Cell::new(format!("{:.2}", r.quantity)),
            Cell::new(running_total_str),
        ]
    }));

    writeln!(out, "{}", table)
}
