use crate::symbol::Symbol;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, Table};
use rust_decimal::Decimal;

pub mod balance {
    use super::*;
    use crate::{
        balance::{AccountBal, Balance},
        commodity::Amount,
    };
    use std::io::{self, Write};

    pub fn print<'a>(mut out: impl Write, balance: &'a Balance, no_total: bool) -> io::Result<()> {
        let mut table = Table::new();
        table.load_preset(presets::NOTHING);

        let mut tot = Amount::new();
        for p in balance.iter_parent() {
            tot += &p.balance;
            print_account_bal(&mut table, p, 0);
        }

        if no_total {
            return writeln!(out, "{}", table);
        };

        table.add_row(vec![Cell::new("--------------")
            .add_attribute(Attribute::Bold)
            .set_alignment(CellAlignment::Right)]);

        if tot.is_zero() {
            table.add_row(vec![Cell::new("0").set_alignment(CellAlignment::Right)]);
            return writeln!(out, "{}", table);
        }

        for (s, q) in tot.iter() {
            table.add_row(vec![
                maybe_colored(s, q).set_alignment(CellAlignment::Right),
                Cell::new(""),
            ]);
        }

        writeln!(out, "{}", table)
    }

    fn print_account_bal(table: &mut Table, accnt: &AccountBal, indent: usize) {
        let qs = accnt.balance.iter().collect::<Vec<(_, _)>>();

        for (s, q) in &qs[..qs.len() - 1] {
            table.add_row(vec![
                maybe_colored(s, q).set_alignment(CellAlignment::Right),
                Cell::new(""),
            ]);
        }

        let (s, q) = qs[qs.len() - 1];
        table.add_row(vec![
            maybe_colored(s, q).set_alignment(CellAlignment::Right),
            Cell::new(format!("{}{}", "  ".repeat(indent), accnt.name))
                .fg(Color::DarkBlue)
                .set_alignment(CellAlignment::Left),
        ]);

        if let Some(subs) = &accnt.sub_account {
            for sub in subs.values() {
                print_account_bal(table, sub, indent + 1);
            }
        }
    }

    /// Returns a `Cell` displaying "{symbol} {value}", colored DarkRed if
    /// `q` is negative.
    fn maybe_colored(s: &Symbol, q: &Decimal) -> Cell {
        let text = format!("{} {:.2}", s, q);
        if *q < Decimal::ZERO {
            Cell::new(text).fg(Color::DarkRed)
        } else {
            Cell::new(text)
        }
    }
}

pub mod register {
    use super::*;
    use crate::register::Register;
    use std::io::{self, Write};

    pub fn print<'a>(
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
                Cell::new(r.account).fg(Color::DarkBlue),
                if r.quantity.q < Decimal::ZERO {
                    Cell::new(format!("{:.2}", r.quantity)).fg(Color::DarkRed)
                } else {
                    Cell::new(format!("{:.2}", r.quantity))
                },
                Cell::new(running_total_str),
            ]
        }));

        writeln!(out, "{}", table)
    }
}
