use std::collections::BTreeMap;

use crate::commodity::{Amount, Quantity};
use crate::journal::AccName;
use comfy_table::{Attribute, Cell, CellAlignment, Color, Table, presets};
use console;
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
    use crate::balance_view::{AccountView, BalanceView, TValue};

    pub fn print<V, T>(
        mut out: impl Write,
        balance: &BalanceView<T>,
        no_total: bool,
        fmt: Fmt,
    ) -> io::Result<()>
    where
        V: TValue,
        T: AccountView<Value = V> + Serialize,
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

    fn print_tty<V: TValue, T: AccountView<Value = V>>(
        mut out: impl Write,
        balance: &BalanceView<T>,
        no_total: bool,
    ) -> io::Result<()> {
        let header = balance
            .balance()
            .iter()
            .map(|(d, _)| {
                Cell::new(d)
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Right)
            })
            .collect::<Vec<_>>();

        let width = header.len();

        let mut table = Table::new();
        table.load_preset(presets::NOTHING).set_header(header);
        table.add_row(vec![
            Cell::new("--------------")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Right);
            width
        ]);

        for p in balance.accounts() {
            print_account_bal(&mut table, p, 0, width);
        }

        if no_total {
            return writeln!(out, "{}", table);
        };

        table.add_row(vec![
            Cell::new("--------------")
                .add_attribute(Attribute::Bold)
                .set_alignment(CellAlignment::Right);
            width
        ]);

        let mut vtot = vec![Cell::new(""); width];
        let tot = balance.balance();
        for (w, a) in tot.iter().map(|(_, amount)| amount).enumerate() {
            if a.is_zero() {
                vtot[w] = Cell::new("0")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Right);
                continue;
            }

            vtot[w] = amount(a, CellAlignment::Right, 0).add_attribute(Attribute::Bold);
        }
        table.add_row(vtot);
        writeln!(out, "{}", table)
    }

    fn print_account_bal(table: &mut Table, accnt: &impl AccountView, indent: usize, width: usize) {
        let heigh = accnt
            .balance()
            .iter()
            .map(|(_, a)| a.arity())
            .max()
            .unwrap_or(1);

        let mut rows = vec![vec![Cell::new(""); width + 1]; heigh];
        for (w, amount) in accnt.balance().iter().map(|(_, amount)| amount).enumerate() {
            if amount.is_zero() {
                rows[0][w] = Cell::new("0").set_alignment(CellAlignment::Right);
                continue;
            }

            for (h, a) in amount.iter_quantities().enumerate() {
                rows[h][w] = quantiry(a, CellAlignment::Right);
            }
        }

        rows.reverse();
        rows[heigh - 1][width] = accont_name(accnt.name(), indent, CellAlignment::Left);
        for row in rows {
            table.add_row(row);
        }

        for sub in accnt.sub_accounts() {
            print_account_bal(table, sub, indent + 1, width);
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
            table.add_row(vec![
                Cell::new(date.to_string()),
                Cell::new(payee),
                accont_name(&entry.acc_name, 0, CellAlignment::Left),
                amount(&entry.total, CellAlignment::Right, 0),
                amount(
                    &entry.running_total,
                    CellAlignment::Right,
                    &entry.total.arity() - 1,
                ),
            ]);
        }

        fn add_row_2p(table: &mut Table, entry: &RegisterEntry) {
            table.add_row(vec![
                Cell::new(""),
                Cell::new(""),
                accont_name(&entry.acc_name, 0, CellAlignment::Left),
                amount(&entry.total, CellAlignment::Right, 0),
                amount(
                    &entry.running_total,
                    CellAlignment::Right,
                    &entry.total.arity() - 1,
                ),
            ]);
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
fn quantiry(q: Quantity, align: CellAlignment) -> Cell {
    let text = format!("{:.2}", q);
    let cell = if q.q < Decimal::ZERO {
        Cell::new(text).fg(Color::DarkRed)
    } else {
        Cell::new(text)
    };

    cell.set_alignment(align)
}

fn amount(q: &Amount, align: CellAlignment, voffset: usize) -> Cell {
    let cell = if q.is_zero() {
        Cell::new("0")
    } else {
        Cell::new(
            std::iter::repeat_n(String::new(), voffset)
                .chain(
                    q.iter_quantities()
                        .map(|q| (format!("{}", q.s), q))
                        .collect::<BTreeMap<_, _>>()
                        .values()
                        .map(|q| {
                            let text = format!("{:.2}", q);
                            if q.q < Decimal::ZERO {
                                console::style(text).red().to_string()
                            } else {
                                text
                            }
                        }),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };

    cell.set_alignment(align)
}
