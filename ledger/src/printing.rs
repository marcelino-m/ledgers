use std::collections::BTreeMap;

use comfy_table::{Attribute, Cell, CellAlignment, Color, Table, presets};
use console;
use rust_decimal::Decimal;
use serde_json;

use crate::balance::Valuation;
use crate::journal::AccName;
use crate::ntypes::{Basket, QValuable, Quantities, Valuable, Zero};
use crate::quantity::Quantity;
pub use balance::TotalMode;
pub use balance::print as bal;
pub use info::print as info;
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
    use crate::account_view::{AccountView, ValuebleAccountView};
    use crate::balance_view::BalanceView;
    use crate::holdings::Holdings;
    use crate::ntypes::{QValuable, TsBasket, Zero};
    use crate::tamount::TAmount;

    /// Controls whether to show account lines and/or the total line
    #[derive(PartialEq)]
    pub enum TotalMode {
        /// Show accounts and total (default)
        Full,
        /// `--no-total`: show accounts, omit the total line
        NoTotal,
        /// `--only-total`: show only the total, omit account lines
        OnlyTotal,
    }

    impl TotalMode {
        pub fn show_tables(&self) -> bool {
            matches!(self, TotalMode::Full | TotalMode::NoTotal)
        }

        pub fn show_total(&self) -> bool {
            matches!(self, TotalMode::Full | TotalMode::OnlyTotal)
        }
    }

    /// Serializers for the `bal` JSON/Lisp output.
    ///
    /// Reorganises the balance tree as `balances → date → {balance, accounts}`
    /// and emits a compact `Holdings` shape (`{symbol: {qty, prices}}`).
    /// No valuation or gain is applied: consumers see raw data and decide.
    mod render {
        use chrono::NaiveDate;
        use serde::ser::{Serialize, SerializeMap, Serializer};
        use std::collections::BTreeSet;

        use crate::account_view::AccountView;
        use crate::balance_view::BalanceView;
        use crate::holdings::{Holdings, Lot};
        use crate::ntypes::TsBasket;
        use crate::tamount::TAmount;

        /// Top-level: `{balances: {date: snapshot}}`.
        pub struct Doc<'a, T: AccountView> {
            pub bv: &'a BalanceView<T>,
            pub only_total: bool,
        }

        impl<T> Serialize for Doc<'_, T>
        where
            T: AccountView<TsValue = TAmount<Holdings>>,
        {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let mut m = ser.serialize_map(Some(1))?;
                m.serialize_entry(
                    "balances",
                    &Balances {
                        bv: self.bv,
                        only_total: self.only_total,
                    },
                )?;
                m.end()
            }
        }

        /// `{date: snapshot}` map. Dates are the union over all accounts.
        struct Balances<'a, T: AccountView> {
            bv: &'a BalanceView<T>,
            only_total: bool,
        }

        impl<T> Serialize for Balances<'_, T>
        where
            T: AccountView<TsValue = TAmount<Holdings>>,
        {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let dates: BTreeSet<NaiveDate> = self
                    .bv
                    .accounts()
                    .flat_map(|a| a.balance().iter_baskets().map(|(d, _)| d))
                    .collect();
                let total = self.bv.balance();

                let mut m = ser.serialize_map(Some(dates.len()))?;
                for d in &dates {
                    m.serialize_entry(
                        d,
                        &Snapshot {
                            date: *d,
                            total: total.at(*d),
                            bv: self.bv,
                            only_total: self.only_total,
                        },
                    )?;
                }
                m.end()
            }
        }

        /// `{balance, accounts?}` for a given date.
        struct Snapshot<'a, T: AccountView> {
            date: NaiveDate,
            total: Option<&'a Holdings>,
            bv: &'a BalanceView<T>,
            only_total: bool,
        }

        impl<T> Serialize for Snapshot<'_, T>
        where
            T: AccountView<TsValue = TAmount<Holdings>>,
        {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let n = if self.only_total { 1 } else { 2 };
                let mut m = ser.serialize_map(Some(n))?;
                m.serialize_entry("balance", &HView(self.total))?;
                if !self.only_total {
                    let accts: Vec<_> = self
                        .bv
                        .accounts()
                        .map(|a| Acct {
                            acc: a,
                            date: self.date,
                        })
                        .collect();
                    m.serialize_entry("accounts", &accts)?;
                }
                m.end()
            }
        }

        /// `{name, balance, sub_account}` for a given date.
        struct Acct<'a, T> {
            acc: &'a T,
            date: NaiveDate,
        }

        impl<T> Serialize for Acct<'_, T>
        where
            T: AccountView<TsValue = TAmount<Holdings>>,
        {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let bal = self.acc.balance().at(self.date);
                let subs: Vec<_> = self
                    .acc
                    .sub_accounts()
                    .map(|a| Acct {
                        acc: a,
                        date: self.date,
                    })
                    .collect();

                let mut m = ser.serialize_map(Some(3))?;
                m.serialize_entry("name", self.acc.name())?;
                m.serialize_entry("balance", &HView(bal))?;
                m.serialize_entry("sub_account", &subs)?;
                m.end()
            }
        }

        /// `{symbol: {qty, prices}}`. Sorted by symbol name.
        struct HView<'a>(Option<&'a Holdings>);

        impl Serialize for HView<'_> {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let mut entries: Vec<_> = self.0.into_iter().flat_map(|h| h.iter_lots()).collect();
                entries.sort_by(|(a, _), (b, _)| a.to_string().cmp(&b.to_string()));

                let mut m = ser.serialize_map(Some(entries.len()))?;
                for (sym, lot) in entries {
                    m.serialize_entry(sym, &LView(lot))?;
                }
                m.end()
            }
        }

        /// `{qty, prices: {market, historical, basis}}`.
        struct LView<'a>(&'a Lot);

        impl Serialize for LView<'_> {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let mut m = ser.serialize_map(Some(2))?;
                m.serialize_entry("qty", &self.0.qty.q)?;
                m.serialize_entry("prices", &Prices(self.0))?;
                m.end()
            }
        }

        struct Prices<'a>(&'a Lot);

        impl Serialize for Prices<'_> {
            fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
                let mut m = ser.serialize_map(Some(3))?;
                m.serialize_entry("market", &self.0.m_uprice)?;
                m.serialize_entry("historical", &self.0.h_uprice)?;
                m.serialize_entry("basis", &self.0.b_uprice)?;
                m.end()
            }
        }
    }

    pub fn print<T>(
        mut out: impl Write,
        balance: &BalanceView<T>,
        total_mode: TotalMode,
        show_detail: Option<Valuation>,
        date_header: bool,
        v: Valuation,
        fmt: Fmt,
    ) -> io::Result<()>
    where
        T: ValuebleAccountView<TsValue = TAmount<Holdings>> + Serialize,
    {
        match fmt {
            Fmt::Tty => print_tty(out, balance, total_mode, show_detail, date_header, v),
            Fmt::Json => {
                let doc = render::Doc {
                    bv: balance,
                    only_total: !total_mode.show_tables(),
                };
                writeln!(out, "{}", serde_json::to_string(&doc)?)
            }
            Fmt::Lisp => {
                let doc = render::Doc {
                    bv: balance,
                    only_total: !total_mode.show_tables(),
                };
                writeln!(out, "{}", serde_lexpr::to_string(&doc)?)
            }
        }
    }

    fn print_tty<V, T>(
        mut out: impl Write,
        balance: &BalanceView<T>,
        total_mode: TotalMode,
        show_detail: Option<Valuation>,
        date_header: bool,
        v: Valuation,
    ) -> io::Result<()>
    where
        V: TsBasket<B: Valuable + QValuable>,
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
        table.load_preset(presets::NOTHING);
        if date_header {
            table.set_header(header);
            table.add_row(vec![
                Cell::new("---------------------")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Right);
                width
            ]);
        }

        if total_mode.show_tables() {
            for p in balance.accounts() {
                print_account_bal(&mut table, p, v, 0, width);
            }
        }

        if total_mode.show_tables() && total_mode.show_total() {
            table.add_row(vec![
                Cell::new("--------------------")
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Right);
                width
            ]);
        }

        if total_mode.show_total() {
            let mut vtot = vec![Cell::new(""); width];
            let tot = balance.balance();
            for (w, a) in tot.iter_baskets().map(|(_, amount)| amount).enumerate() {
                vtot[w] = amount2(a, v, show_detail, CellAlignment::Right, 0)
                    .add_attribute(Attribute::Bold);
            }
            table.add_row(vtot);
        }

        writeln!(out, "{}", table)
    }

    fn print_account_bal<V, T>(
        table: &mut Table,
        accnt: &T,
        v: Valuation,
        indent: usize,
        width: usize,
    ) where
        V: TsBasket<B: Valuable + QValuable>,
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
                rows[0][w] =
                    Cell::new(format!("{:>20.1}", 0.0)).set_alignment(CellAlignment::Right);
                continue;
            }

            for (h, a) in amount.quantities().enumerate() {
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
    use crate::register::RegisterGroup;
    use crate::register::RegisterRow;

    pub fn print<'a>(
        mut out: impl Write,
        reg: impl Iterator<Item = RegisterGroup<'a>>,
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
        reg: impl Iterator<Item = RegisterGroup<'a>>,
    ) -> io::Result<()> {
        let mut table = Table::new();
        table.load_preset(presets::NOTHING).set_header(
            ["Date", "Payee", "Account", "Amount", "RunningTotal"].map(|s| {
                Cell::new(s)
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center)
            }),
        );

        fn add_row_1(table: &mut Table, date: NaiveDate, payee: &str, entry: &RegisterRow) {
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

        fn add_row_2p(table: &mut Table, entry: &RegisterRow) {
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
            let (row, left_rows) = r.rows.split_first().unwrap();

            add_row_1(&mut table, *r.date, r.payee, row);
            for row in left_rows {
                add_row_2p(&mut table, row);
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
    let text = format!("{:>20.1}", q);
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
        Cell::new(format!("{:>20.1}", 0.0))
    } else {
        Cell::new(
            std::iter::repeat_n(String::new(), voffset)
                .chain(
                    amt.quantities()
                        .map(|q| (format!("{}", q.s), q))
                        .collect::<BTreeMap<_, _>>() // to sort for name of commodity
                        .values()
                        .map(|q| {
                            let qty = format!("{:>20.1}", q);
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
    let vamt = amt.valued_in(v);
    let cell = if vamt.is_zero() {
        // amt could be non zero, but amt.valuded_in(v) could be
        Cell::new("0")
    } else {
        Cell::new(
            std::iter::repeat_n(String::new(), voffset)
                .chain(
                    vamt.quantities()
                        .map(|q| (format!("{}", q.s), q))
                        .collect::<BTreeMap<_, _>>() // to sort for name of commodity
                        .values()
                        .map(|q| {
                            let qty = format!("{:>20.1}", q);
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
                                        console::style(format!("+{:.1}%", g)).green().to_string()
                                    } else if g < Decimal::ZERO {
                                        console::style(format!("{:.1}%", g)).red().to_string()
                                    } else {
                                        " 0.00%".to_string()
                                    }
                                })
                            };

                            let price = amt
                                .svalued_in(q.s, pv)
                                .quantities()
                                .filter(|b| b.s != q.s)
                                .map(|b| format!("{:>20.1}", b))
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

mod info {
    use std::io::{self, Write};

    use super::*;
    use crate::info::JournalReport;

    pub fn print(mut out: impl Write, report: &JournalReport, fmt: Fmt) -> io::Result<()> {
        match fmt {
            Fmt::Json => writeln!(out, "{}", serde_json::to_string(report).unwrap()),
            Fmt::Lisp => writeln!(out, "{}", serde_lexpr::to_string(report).unwrap()),
            Fmt::Tty => print_tty(out, report),
        }
    }

    fn print_tty(mut out: impl Write, report: &JournalReport) -> io::Result<()> {
        writeln!(out, "Accounts:")?;
        for acc in &report.accounts {
            writeln!(out, "  {acc}")?;
        }
        writeln!(out, "")?;
        writeln!(out, "Commodities:")?;
        for c in &report.commodities {
            writeln!(out, "  {c}")?;
        }
        Ok(())
    }
}
