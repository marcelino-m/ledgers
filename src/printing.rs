use crate::commodity::Quantity;
use crate::journal::AccName;
use comfy_table::{Attribute, Cell, CellAlignment, Color, Table, presets};
use rust_decimal::Decimal;
use serde_json;

pub use balance::print as bal;
pub use register::print as reg;

/// Output format of the report
pub enum Fmt {
    Tty,
    Json,
    Lisp,
}

mod balance {
    use std::io::{self, Write};

    use serde::Serialize;

    use super::*;
    use crate::balance::{Account, Balance};

    pub fn print<'a, T>(
        mut out: impl Write,
        balance: &'a Balance<T>,
        no_total: bool,
        fmt: Fmt,
    ) -> io::Result<()>
    where
        T: Account + Serialize,
    {
        match fmt {
            Fmt::Tty => print_tty(out, balance, no_total),
            Fmt::Json => {
                writeln!(out, "{}", serde_json::to_string(balance).unwrap())
            }
            Fmt::Lisp => {
                writeln!(out, "{}", serde_lexpr::to_string(balance).unwrap())
            }
        }
    }

    fn print_tty<'a, T: Account>(
        mut out: impl Write,
        balance: &'a Balance<T>,
        no_total: bool,
    ) -> io::Result<()> {
        let mut table = Table::new();
        table.load_preset(presets::NOTHING);

        for p in balance.accounts() {
            print_account_bal(&mut table, p, 0);
        }

        if no_total {
            return writeln!(out, "{}", table);
        };

        table.add_row(vec![
            Cell::new("--------------")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Right),
        ]);

        let tot = balance.balance();
        if tot.is_zero() {
            table.add_row(vec![Cell::new("0").set_alignment(CellAlignment::Right)]);
            return writeln!(out, "{}", table);
        }

        for qty in tot.iter_quantities() {
            table.add_row(vec![commodity(qty, CellAlignment::Right), Cell::new("")]);
        }

        writeln!(out, "{}", table)
    }

    fn print_account_bal(table: &mut Table, accnt: &impl Account, indent: usize) {
        let is_zero = accnt.balance().is_zero();
        if is_zero {
            table.add_row(vec![
                Cell::new("0").set_alignment(CellAlignment::Right),
                accont_name(accnt.name(), indent, CellAlignment::Left),
            ]);
        } else {
            let qtys = accnt.balance().iter_quantities().collect::<Vec<_>>();

            for qty in &qtys[..qtys.len() - 1] {
                table.add_row(vec![commodity(*qty, CellAlignment::Right), Cell::new("")]);
            }

            let qty = qtys[qtys.len() - 1];
            table.add_row(vec![
                commodity(qty, CellAlignment::Right),
                accont_name(accnt.name(), indent, CellAlignment::Left),
            ]);
        }

        for sub in accnt.sub_accounts() {
            print_account_bal(table, sub, indent + 1);
        }
    }
}

mod register {
    use std::io::{self, Write};

    use chrono::NaiveDate;

    use super::*;
    use crate::register::Register;
    use crate::register::RegisterEntry;

    pub fn print<'a>(
        mut out: impl Write,
        reg: impl Iterator<Item = Register<'a>>,
        fmt: Fmt,
    ) -> io::Result<()> {
        match fmt {
            Fmt::Tty => print_tty(out, reg),
            Fmt::Json => {
                let reg = reg.collect::<Vec<_>>();
                writeln!(out, "{}", serde_json::to_string(&reg).unwrap())
            }
            Fmt::Lisp => {
                let reg = reg.collect::<Vec<_>>();
                writeln!(out, "{}", serde_lexpr::to_string(&reg).unwrap())
            }
        }
    }

    fn print_tty<'a>(
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

        fn add_row_1(table: &mut Table, date: NaiveDate, payee: &str, entry: &RegisterEntry) {
            if entry.running_total.is_zero() {
                table.add_row(vec![
                    Cell::new(date.to_string()),
                    Cell::new(payee),
                    accont_name(entry.acc_name, 0, CellAlignment::Left),
                    commodity(entry.quantity, CellAlignment::Right),
                    Cell::new("0").set_alignment(CellAlignment::Right),
                ]);
            } else {
                let mut iter = entry.running_total.iter_quantities();
                let qty = iter.next().unwrap();
                table.add_row(vec![
                    Cell::new(date.to_string()),
                    Cell::new(payee),
                    accont_name(entry.acc_name, 0, CellAlignment::Left),
                    commodity(entry.quantity, CellAlignment::Right),
                    commodity(qty, CellAlignment::Right),
                ]);

                for qty in iter {
                    table.add_row(vec![
                        Cell::new(""),
                        Cell::new(""),
                        Cell::new(""),
                        Cell::new(""),
                        commodity(qty, CellAlignment::Right),
                    ]);
                }
            }
        }

        fn add_row_2p(table: &mut Table, entry: &RegisterEntry) {
            if entry.running_total.is_zero() {
                table.add_row(vec![
                    Cell::new(""),
                    Cell::new(""),
                    accont_name(entry.acc_name, 0, CellAlignment::Left),
                    commodity(entry.quantity, CellAlignment::Right),
                    Cell::new("0").set_alignment(CellAlignment::Right),
                ]);
            } else {
                let mut iter = entry.running_total.iter_quantities();
                let qty = iter.next().unwrap();
                table.add_row(vec![
                    Cell::new(""),
                    Cell::new(""),
                    accont_name(entry.acc_name, 0, CellAlignment::Left),
                    commodity(entry.quantity, CellAlignment::Right),
                    commodity(qty, CellAlignment::Right),
                ]);

                for qty in iter {
                    table.add_row(vec![
                        Cell::new(""),
                        Cell::new(""),
                        Cell::new(""),
                        Cell::new(""),
                        commodity(qty, CellAlignment::Right),
                    ]);
                }
            }
        }

        for r in reg {
            let (fentry, rentries) = r.entries.split_first().unwrap();

            add_row_1(&mut table, *r.date, r.payee, fentry);
            for e in rentries {
                add_row_2p(&mut table, e);
            }
        }

        match writeln!(out, "{}", table) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(e),
        }
    }
}

/// Returns a `Cell` displaying the account name indented
fn accont_name(n: &AccName, indent: usize, align: CellAlignment) -> Cell {
    Cell::new(format!("{}{}", "  ".repeat(indent), n))
        .fg(Color::DarkBlue)
        .set_alignment(align)
}

/// Returns a `Cell` displaying "{symbol} {value}", colored DarkRed if
/// `q` is negative.
fn commodity(q: Quantity, align: CellAlignment) -> Cell {
    let text = format!("{:.2}", q);
    let cell = if q.q < Decimal::ZERO {
        Cell::new(text).fg(Color::DarkRed)
    } else {
        Cell::new(text)
    };

    cell.set_alignment(align)
}
