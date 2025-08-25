use crate::symbol::Symbol;
use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, Table};
use rust_decimal::Decimal;

pub mod balance {
    use super::*;
    use crate::{
        account::AccountName,
        balance::{AccountBal, Balance},
        commodity::Amount,
    };
    use std::io::{self, Write};

    pub fn print<'a>(
        mut out: impl Write,
        balance: &'a Balance,
        no_total: bool,
        print_empyt: bool,
    ) -> io::Result<()> {
        let mut table = Table::new();
        table.load_preset(presets::NOTHING);

        let mut tot = Amount::new();
        for p in balance.iter_parent() {
            tot += &p.balance;
            print_account_bal(&mut table, p, 0, print_empyt);
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
            table.add_row(vec![commodity(s, q, CellAlignment::Right), Cell::new("")]);
        }

        writeln!(out, "{}", table)
    }

    fn print_account_bal(table: &mut Table, accnt: &AccountBal, indent: usize, print_empyt: bool) {
        let is_zero = accnt.balance.is_zero();
        if is_zero && !print_empyt {
            return;
        }

        if is_zero {
            table.add_row(vec![
                Cell::new("0").set_alignment(CellAlignment::Right),
                accont_name(&accnt.name, indent, CellAlignment::Left),
            ]);
            return;
        }

        let qs = accnt.balance.iter().collect::<Vec<(_, _)>>();

        for (s, q) in &qs[..qs.len() - 1] {
            table.add_row(vec![commodity(s, q, CellAlignment::Right), Cell::new("")]);
        }

        let (s, q) = qs[qs.len() - 1];
        table.add_row(vec![
            commodity(s, q, CellAlignment::Right),
            accont_name(&accnt.name, indent, CellAlignment::Left),
        ]);

        if let Some(subs) = &accnt.sub_account {
            for sub in subs.values() {
                print_account_bal(table, sub, indent + 1, print_empyt);
            }
        }
    }

    /// Returns a `Cell` displaying the account name indented
    fn accont_name(n: &AccountName, indent: usize, align: CellAlignment) -> Cell {
        Cell::new(format!("{}{}", "  ".repeat(indent), n))
            .fg(Color::DarkBlue)
            .set_alignment(align)
    }

    /// Returns a `Cell` displaying "{symbol} {value}", colored DarkRed if
    /// `q` is negative.
    fn commodity(s: &Symbol, q: &Decimal, align: CellAlignment) -> Cell {
        let text = format!("{} {:.2}", s, q);
        let cell = if *q < Decimal::ZERO {
            Cell::new(text).fg(Color::DarkRed)
        } else {
            Cell::new(text)
        };

        cell.set_alignment(align)
    }
}

pub mod register {
    use super::*;
    use crate::commodity::Amount;
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

        let fmt_amt = |amt: &Amount| {
            amt.iter()
                .map(|(s, q)| format!("{} {:.2}", s, q))
                .collect::<Vec<_>>()
                .join("\n")
        };

        for r in reg {
            let (first, rest) = r.entries.split_first().unwrap();

            table.add_row(vec![
                Cell::new(r.date.to_string()),
                Cell::new(r.payee),
                Cell::new(first.account).fg(Color::DarkBlue),
                if first.quantity.q < Decimal::ZERO {
                    Cell::new(format!("{:.2}", first.quantity)).fg(Color::DarkRed)
                } else {
                    Cell::new(format!("{:.2}", first.quantity))
                },
                Cell::new(fmt_amt(&first.running_total)),
            ]);

            for e in rest {
                table.add_row(vec![
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(e.account).fg(Color::DarkBlue),
                    if e.quantity.q < Decimal::ZERO {
                        Cell::new(format!("{:.2}", e.quantity)).fg(Color::DarkRed)
                    } else {
                        Cell::new(format!("{:.2}", e.quantity))
                    },
                    Cell::new(fmt_amt(&e.running_total)),
                ]);
            }
        }
        writeln!(out, "{}", table)
    }
}
