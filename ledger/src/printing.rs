use std::collections::BTreeMap;

use comfy_table::{presets, Attribute, Cell, CellAlignment, Color, Table};
use console;
use rust_decimal::Decimal;
use serde_json;

use crate::balance::Valuation;
use crate::journal::AccName;
use crate::ntypes::{Basket, QValuable, Valuable, Zero};
use crate::quantity::Quantity;
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
    use crate::balance_view::{AccountView, BalanceView, ValuebleAccountView};
    use crate::ntypes::{QValuable, TsBasket, Zero};

    pub fn print<V, T>(
        mut out: impl Write,
        balance: &BalanceView<T>,
        no_total: bool,
        show_detail: Option<Valuation>,
        v: Valuation,
        fmt: Fmt,
    ) -> io::Result<()>
    where
        V: TsBasket<B: Valuable + QValuable> + Zero,
        T: ValuebleAccountView<TsValue = V> + Serialize,
    {
        match fmt {
            Fmt::Tty => print_tty(out, balance, no_total, show_detail, v),
            Fmt::Json => {
                writeln!(out, "{}", serde_json::to_string(balance).unwrap())
            }
            Fmt::Lisp => {
                writeln!(out, "{}", serde_lexpr::to_string(balance).unwrap())
            }
        }
    }

    fn print_tty<V, T>(
        mut out: impl Write,
        balance: &BalanceView<T>,
        no_total: bool,
        show_detail: Option<Valuation>,
        v: Valuation,
    ) -> io::Result<()>
    where
        V: TsBasket<B: Valuable + QValuable> + Zero,
        T: ValuebleAccountView<TsValue = V>,
    {
        // contain the dates of balances
        let header = balance
            .balance()
            .iter_baskets()
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
            print_account_bal(&mut table, p, v, 0, width);
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
        for (w, a) in tot.iter_baskets().map(|(_, amount)| amount).enumerate() {
            if a.is_zero() {
                vtot[w] = Cell::new("0")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Right);
                continue;
            }

            vtot[w] =
                amount2(a, v, show_detail, CellAlignment::Right, 0).add_attribute(Attribute::Bold);
        }
        table.add_row(vtot);
        writeln!(out, "{}", table)
    }

    fn print_account_bal<V, T>(
        table: &mut Table,
        accnt: &T,
        v: Valuation,
        indent: usize,
        width: usize,
    ) where
        V: TsBasket<B: Valuable + QValuable> + Zero,
        T: ValuebleAccountView<TsValue = V>,
    {
        let accnt_v = accnt.valued_in(v);
        // The display height equals the maximum number of commodity (arity)
        // of this account’s balance over time.
        let heigh = accnt_v
            .balance()
            .iter_baskets()
            .map(|(_date, a)| a.arity())
            .chain(std::iter::once(1)) // could be zero-height due to zero balances
            .max()
            .unwrap();

        let mut rows = vec![vec![Cell::new(""); width + 1]; heigh];
        for (w, amount) in accnt_v
            .balance()
            .iter_baskets()
            .map(|(_, amount)| amount)
            .enumerate()
        {
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
            print_account_bal(table, sub, v, indent + 1, width);
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
                    if entry.total.is_zero() {
                        0
                    } else {
                        &entry.total.arity() - 1
                    },
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
                    if entry.total.is_zero() {
                        0
                    } else {
                        &entry.total.arity() - 1
                    },
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

fn amount<V>(amt: &V, align: CellAlignment, voffset: usize) -> Cell
where
    V: Basket + Valuable,
{
    let cell = if amt.is_zero() {
        Cell::new("0")
    } else {
        Cell::new(
            std::iter::repeat_n(String::new(), voffset)
                .chain(
                    amt.iter_quantities()
                        .map(|q| (format!("{}", q.s), q))
                        .collect::<BTreeMap<_, _>>() // to sort for name of commodity
                        .values()
                        .map(|q| {
                            let qty = format!("{:.2}", q);
                            if q.q < Decimal::ZERO {
                                console::style(qty).red().to_string()
                            } else {
                                qty
                            }
                        }),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };

    cell.set_alignment(align)
}

fn amount2<V>(
    amt: &V,
    v: Valuation,
    show_detail: Option<Valuation>,
    align: CellAlignment,
    voffset: usize,
) -> Cell
where
    V: Basket + Valuable + QValuable,
{
    let cell = if amt.is_zero() {
        Cell::new("0")
    } else {
        Cell::new(
            std::iter::repeat_n(String::new(), voffset)
                .chain(
                    amt.valued_in(v)
                        .iter_quantities()
                        .map(|q| (format!("{}", q.s), q))
                        .collect::<BTreeMap<_, _>>() // to sort for name of commodity
                        .values()
                        .map(|q| {
                            let qty = format!("{:.2}", q);
                            let qty = if q.q < Decimal::ZERO {
                                console::style(qty).red().to_string()
                            } else {
                                qty
                            };

                            let Some(pv) = show_detail else {
                                return qty;
                            };

                            let gain = {
                                let empty = "      ".to_string();
                                amt.sgain(q.s, pv).map_or(empty.clone(), |(mut g, s)| {
                                    if s == q.s {
                                        return empty;
                                    }

                                    g *= Decimal::from(100);
                                    if g > Decimal::ZERO {
                                        console::style(format!("+{:.2}%", g)).green().to_string()
                                    } else if g < Decimal::ZERO {
                                        console::style(format!("{:.2}%", g)).red().to_string()
                                    } else {
                                        " 0.00%".to_string()
                                    }
                                })
                            };

                            let price = amt
                                .svalued_in(q.s, pv)
                                .iter_quantities()
                                .filter(|b| b.s != q.s)
                                .map(|b| format!("{:.2}", b))
                                .collect::<Vec<_>>()
                                .join(", ");

                            let price = console::style(price).true_color(128, 128, 128).to_string();

                            format!("{:>18} {:<5} {:>20}", price, gain, qty)
                        }),
                )
                .collect::<Vec<_>>()
                .join("\n"),
        )
    };

    cell.set_alignment(align)
}
