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
pub use print::print as prnt;
pub use register::print as reg;

/// Wire format for the atom types (`Symbol`, `AccName`, `Quantity`,
/// `Amount`).
mod prims {
    use schemars::JsonSchema;
    use schemars::r#gen::SchemaGenerator;
    use schemars::schema::{InstanceType, Metadata, ObjectValidation, Schema, SchemaObject};
    use serde::Serialize;
    use serde::ser::{SerializeMap, Serializer};

    use crate::amount::Amount;
    use crate::journal::AccName;
    use crate::ntypes::{Basket, Quantities};
    use crate::quantity::Quantity;
    use crate::symbol::Symbol;

    impl Serialize for Symbol {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            ser.serialize_str(&self.name())
        }
    }

    impl JsonSchema for Symbol {
        fn schema_name() -> String {
            "Symbol".to_owned()
        }

        fn json_schema(g: &mut SchemaGenerator) -> Schema {
            // E.g. "$", "AAPL", "USD".
            String::json_schema(g)
        }
    }

    impl Serialize for AccName {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            ser.serialize_str(self)
        }
    }

    impl JsonSchema for AccName {
        fn schema_name() -> String {
            "AccName".to_owned()
        }

        fn json_schema(g: &mut SchemaGenerator) -> Schema {
            String::json_schema(g)
        }
    }

    impl Serialize for Quantity {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            let mut map = ser.serialize_map(Some(1))?;
            map.serialize_entry(&self.s, &self.q)?;
            map.end()
        }
    }

    impl JsonSchema for Quantity {
        fn schema_name() -> String {
            "Quantity".to_owned()
        }

        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            SchemaObject {
                instance_type: Some(InstanceType::Object.into()),
                object: Some(Box::new(ObjectValidation {
                    additional_properties: Some(Box::new(decimal_string_schema().into())),
                    min_properties: Some(1),
                    max_properties: Some(1),
                    ..Default::default()
                })),
                metadata: Some(Box::new(Metadata {
                    description: Some(
                        "A quantity of a single commodity, encoded as a one-entry map \
                         { commodity_symbol: decimal_string }."
                            .to_owned(),
                    ),
                    ..Default::default()
                })),
                ..Default::default()
            }
            .into()
        }
    }

    impl Serialize for Amount {
        fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
            let mut map = ser.serialize_map(Some(self.arity()))?;
            for q in self.quantities() {
                map.serialize_entry(&q.s, &q.q)?;
            }
            map.end()
        }
    }

    impl JsonSchema for Amount {
        fn schema_name() -> String {
            "Amount".to_owned()
        }

        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            SchemaObject {
                instance_type: Some(InstanceType::Object.into()),
                object: Some(Box::new(ObjectValidation {
                    additional_properties: Some(Box::new(decimal_string_schema().into())),
                    ..Default::default()
                })),
                metadata: Some(Box::new(Metadata {
                    description: Some(
                        "Multi-commodity amount, encoded as a map \
                         { commodity_symbol: decimal_string }. An empty map represents a \
                         zero amount."
                            .to_owned(),
                    ),
                    ..Default::default()
                })),
                ..Default::default()
            }
            .into()
        }
    }

    /// Schema body shared by every `Decimal` field on the wire.
    pub fn decimal_string_schema() -> SchemaObject {
        SchemaObject {
            instance_type: Some(InstanceType::String.into()),
            format: Some("decimal".to_owned()),
            metadata: Some(Box::new(Metadata {
                description: Some(
                    "Arbitrary-precision decimal serialized as a string \
                     (e.g. \"123.45\", \"-0.001\")."
                        .to_owned(),
                ),
                ..Default::default()
            })),
            ..Default::default()
        }
    }

    /// `schema_with` adapter for `Decimal` fields in derive-based wire types.
    ///
    /// Use as `#[schemars(schema_with = "crate::printing::leaf::decimal_string_schema_fn")]`.
    pub fn decimal_string_schema_fn(_: &mut SchemaGenerator) -> Schema {
        decimal_string_schema().into()
    }
}

/// Output format of the report
pub enum Fmt {
    Tty,
    Json,
    Lisp,
}

/// Schema selector for the `schema` subcommand. Each variant maps 1:1
/// to a report whose `--fmt json` shape we expose.
#[derive(Clone, Copy, Debug, clap::ValueEnum)]
pub enum Schema {
    #[value(alias = "bal")]
    Balance,
    #[value(alias = "reg")]
    Register,
    #[value(alias = "inf")]
    Info,
    #[value(alias = "pr")]
    Print,
}

impl std::fmt::Display for Schema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Schema::Balance => "balance",
            Schema::Register => "register",
            Schema::Info => "info",
            Schema::Print => "print",
        })
    }
}

/// Print the JSON schema for the `--fmt json` output of a report to
/// `out`, or list available schemas when `which` is `None`.
pub fn schema(mut out: impl std::io::Write, which: Option<Schema>) -> Result<(), String> {
    use clap::ValueEnum;
    use schemars::schema_for;

    let Some(which) = which else {
        for s in Schema::value_variants() {
            writeln!(out, "{s}").map_err(|e| e.to_string())?;
        }
        return Ok(());
    };

    let pretty = match which {
        Schema::Balance => {
            serde_json::to_string_pretty(&schema_for!(balance::wire::BalanceViewWired<'static>))
        }
        Schema::Register => {
            serde_json::to_string_pretty(&schema_for!(register::wire::RegisterReport<'static>))
        }
        Schema::Info => serde_json::to_string_pretty(&schema_for!(info::wire::InfoReport<'static>)),
        Schema::Print => {
            serde_json::to_string_pretty(&schema_for!(print::wire::PrintReport<'static>))
        }
    }
    .map_err(|e| format!("failed to serialize schema: {e}"))?;

    writeln!(out, "{pretty}").map_err(|e| e.to_string())?;
    Ok(())
}

pub mod balance {
    use std::io::{self, Write};

    use super::*;
    use crate::account_view::{AccountView, ValuebleAccountView};
    use crate::balance_view::BalanceView;
    use crate::holdings::Holdings;
    use crate::ntypes::{QValuable, TsBasket, Zero};
    use crate::tamount::TAmount;

    /// Controls whether to show account lines and/or the total line
    #[derive(PartialEq, Copy, Clone)]
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

    /// Stable JSON/Lisp shape for the `balance` report.
    pub mod wire {
        use std::borrow::Cow;
        use std::collections::BTreeMap;

        use chrono::NaiveDate;
        use rust_decimal::Decimal;
        use schemars::JsonSchema;
        use serde::Serialize;

        use crate::account_view::AccountView;
        use crate::amount::Amount;
        use crate::balance_view::BalanceView;
        use crate::holdings::{AvgPosition, Holdings};
        use crate::journal::AccName;
        use crate::ntypes::TsBasket;
        use crate::tamount::TAmount;

        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "BalanceReport")]
        pub struct BalanceViewWired<'a> {
            /// Aggregate balance across every account, as a time-series.
            /// Omitted under `--no-total`.
            #[serde(skip_serializing_if = "Option::is_none")]
            pub balance: Option<BalanceWire<'a>>,
            /// Per-account breakdown. Omitted under `--only-total`.
            #[serde(skip_serializing_if = "Option::is_none")]
            pub accounts: Option<Vec<AccountWire<'a>>>,
        }

        /// One node of the (possibly hierarchical) account tree.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "Account")]
        pub struct AccountWire<'a> {
            /// Account name. Format depends on the display mode:
            /// - `--flat`: the full colon-path (e.g. `Assets:Bank:Checking`).
            /// - default (compact): the path segment from this account's
            ///   direct parent in the tree. May itself contain `:` when
            ///   intermediate single-child parents were collapsed into it
            ///   (e.g. a parent `Assets` may have a child named
            ///   `Provida:Dos` if `Assets:Provida` had only one child).
            /// To reconstruct the full path under compact mode, concatenate
            /// the parent's full path with `:` and this `name`.
            pub name: &'a AccName,
            /// Time-series of this account's balance.
            pub balance: BalanceWire<'a>,
            /// Sub-accounts under this one. Empty under `--flat` or for leaves.
            pub sub_account: Vec<AccountWire<'a>>,
        }

        /// A balance as a time-series: map of evaluation date to value.
        /// Sparse — only the dates where the source has data appear.
        #[derive(Serialize, JsonSchema)]
        #[serde(transparent)]
        #[schemars(rename = "Balance")]
        pub struct BalanceWire<'a>(pub BTreeMap<NaiveDate, ValueWire<'a>>);

        /// The actual balance value at one date. Serialised transparently
        /// (`#[serde(untagged)]`): the variant tag never appears.
        #[derive(Serialize, JsonSchema)]
        #[serde(untagged)]
        #[schemars(rename = "Value")]
        pub enum ValueWire<'a> {
            /// Raw multi-commodity holdings with the three valuation prices.
            HoldingVal(HoldingsWire<'a>),
            /// Valued amount (a possibly multi-commodity map of commodity
            /// to decimal).
            AmountVal(Cow<'a, Amount>),
        }

        /// Multi-commodity holdings, keyed by commodity symbol.
        /// An empty map represents a zero balance.
        #[derive(Serialize, JsonSchema)]
        #[serde(transparent)]
        #[schemars(rename = "Holdings")]
        pub struct HoldingsWire<'a>(pub BTreeMap<String, PositionWire<'a>>);

        impl<'a> From<&'a Holdings> for HoldingsWire<'a> {
            fn from(h: &'a Holdings) -> Self {
                HoldingsWire(
                    h.iter_positions()
                        .map(|(sym, lot)| (sym.to_string(), PositionWire::from(lot)))
                        .collect(),
                )
            }
        }

        /// Holdings of a single commodity together with the prices used
        /// for each valuation method.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "Lot")]
        pub struct PositionWire<'a> {
            /// Quantity held (signed). Serialized as a decimal string.
            #[schemars(schema_with = "crate::printing::prims::decimal_string_schema_fn")]
            pub qty: Decimal,
            /// Unit prices: one per valuation method.
            pub prices: PricesWire<'a>,
        }

        impl<'a> From<&'a AvgPosition> for PositionWire<'a> {
            fn from(lot: &'a AvgPosition) -> Self {
                PositionWire {
                    qty: lot.qty.q,
                    prices: PricesWire {
                        market: &lot.m_uprice,
                        historical: &lot.h_uprice,
                        basis: &lot.b_uprice,
                    },
                }
            }
        }

        /// Unit prices of this position under each valuation method.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "Prices")]
        pub struct PricesWire<'a> {
            /// Current market price (used under `--market`).
            pub market: &'a Amount,
            /// Price at acquisition time (used under `--historical`).
            pub historical: &'a Amount,
            /// Cost basis / book unit price (used under `--basis`).
            pub basis: &'a Amount,
        }

        impl<'a> BalanceViewWired<'a> {
            /// Build a raw report (every leaf is [`ValueWire::HoldingVal`]).
            pub fn from_raw<T>(
                view: &'a BalanceView<T>,
                total: &'a T::TsValue,
                total_mode: super::TotalMode,
            ) -> Self
            where
                T: AccountView<TsValue = TAmount<Holdings>>,
            {
                BalanceViewWired {
                    balance: total_mode.show_total().then(|| raw_balance(total)),
                    accounts: total_mode
                        .show_tables()
                        .then(|| view.accounts().map(raw_account).collect()),
                }
            }

            /// Build a valued report (every leaf is [`ValueWire::AmountVal`]).
            pub fn from_valued<T>(
                view: &'a BalanceView<T>,
                total: &'a T::TsValue,
                total_mode: super::TotalMode,
            ) -> Self
            where
                T: AccountView,
                T::TsValue: TsBasket<B = Amount>,
            {
                BalanceViewWired {
                    balance: total_mode.show_total().then(|| valued_balance(total)),
                    accounts: total_mode
                        .show_tables()
                        .then(|| view.accounts().map(valued_account).collect()),
                }
            }
        }

        fn raw_balance(t: &TAmount<Holdings>) -> BalanceWire<'_> {
            BalanceWire(
                t.iter_baskets()
                    .map(|(d, h)| (d, ValueWire::HoldingVal(HoldingsWire::from(h))))
                    .collect(),
            )
        }

        fn raw_account<T>(acc: &T) -> AccountWire<'_>
        where
            T: AccountView<TsValue = TAmount<Holdings>>,
        {
            AccountWire {
                name: acc.name(),
                balance: raw_balance(acc.balance()),
                sub_account: acc.sub_accounts().map(raw_account).collect(),
            }
        }

        fn valued_balance<B: TsBasket<B = Amount>>(t: &B) -> BalanceWire<'_> {
            BalanceWire(
                t.iter_baskets()
                    .map(|(d, a)| (d, ValueWire::AmountVal(Cow::Borrowed(a))))
                    .collect(),
            )
        }

        fn valued_account<T>(acc: &T) -> AccountWire<'_>
        where
            T: AccountView,
            T::TsValue: TsBasket<B = Amount>,
        {
            AccountWire {
                name: acc.name(),
                balance: valued_balance(acc.balance()),
                sub_account: acc.sub_accounts().map(valued_account).collect(),
            }
        }
    }

    pub fn print<T>(
        out: impl Write,
        balance: &BalanceView<T>,
        total_mode: TotalMode,
        show_detail: Option<Valuation>,
        date_header: bool,
        v: Valuation,
        fmt: Fmt,
    ) -> io::Result<()>
    where
        T: ValuebleAccountView<TsValue = TAmount<Holdings>>,
    {
        if let Fmt::Tty = fmt {
            return print_tty(out, balance, total_mode, show_detail, date_header, v);
        }
        match v {
            Valuation::Quantity => {
                let total = balance.balance();
                let doc = wire::BalanceViewWired::from_raw(balance, &total, total_mode);
                write_doc(out, fmt, &doc)
            }
            _ => {
                let valued = balance.valued_in(v);
                let total = valued.balance();
                let doc = wire::BalanceViewWired::from_valued(&valued, &total, total_mode);
                write_doc(out, fmt, &doc)
            }
        }
    }

    fn write_doc(mut out: impl Write, fmt: Fmt, doc: &impl serde::Serialize) -> io::Result<()> {
        match fmt {
            Fmt::Json => writeln!(out, "{}", serde_json::to_string(doc)?),
            Fmt::Lisp => writeln!(out, "{}", serde_lexpr::to_string(doc)?),
            Fmt::Tty => unreachable!("tty handled before dispatch"),
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

pub mod register {
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
                let groups: Vec<RegisterGroup<'a>> = reg.collect();
                let doc = wire::RegisterReport::from_groups(&groups);
                writeln!(out, "{}", serde_json::to_string(&doc).unwrap())
            }
            Fmt::Lisp => {
                let groups: Vec<RegisterGroup<'a>> = reg.collect();
                let doc = wire::RegisterReport::from_groups(&groups);
                writeln!(out, "{}", serde_lexpr::to_string(&doc).unwrap())
            }
        }
    }

    /// Stable JSON/Lisp shape for the `register` report.
    pub mod wire {
        use chrono::NaiveDate;
        use schemars::JsonSchema;
        use serde::Serialize;

        use crate::amount::Amount;
        use crate::journal::AccName;
        use crate::register::{RegisterGroup, RegisterRow};

        /// Top-level shape of the `register --fmt json` report.
        ///
        /// One [`RegisterGroupWire`] per transaction, in
        /// chronological order.  When the query matches nothing the
        /// report is `[]` and the exit code is `0`.
        #[derive(Serialize, JsonSchema)]
        #[serde(transparent)]
        #[schemars(rename = "RegisterReport")]
        pub struct RegisterReport<'a>(pub Vec<RegisterGroupWire<'a>>);

        impl<'a> RegisterReport<'a> {
            /// Build a `RegisterReport` borrowing from a slice of register groups.
            pub fn from_groups(groups: &'a [RegisterGroup<'a>]) -> Self {
                RegisterReport(groups.iter().map(RegisterGroupWire::from).collect())
            }
        }

        /// All rows contributed by a single transaction.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "RegisterGroup")]
        pub struct RegisterGroupWire<'a> {
            /// Numeric id of the transaction.
            #[serde(rename = "xact-id")]
            pub xact_id: usize,
            /// Transaction date.
            pub date: &'a NaiveDate,
            /// Transaction payee.
            pub payee: &'a str,
            /// Posting rows for this transaction, in display order.
            pub rows: Vec<RegisterRowWire<'a>>,
        }

        impl<'a> From<&'a RegisterGroup<'a>> for RegisterGroupWire<'a> {
            fn from(g: &'a RegisterGroup<'a>) -> Self {
                RegisterGroupWire {
                    xact_id: g.id,
                    date: g.date,
                    payee: g.payee,
                    rows: g.rows.iter().map(RegisterRowWire::from).collect(),
                }
            }
        }

        /// A single row of the register: one posting (or a depth-collapsed bucket).
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "RegisterRow")]
        pub struct RegisterRowWire<'a> {
            pub acc_name: &'a AccName,
            pub total: &'a Amount,
            pub running_total: &'a Amount,
        }

        impl<'a> From<&'a RegisterRow> for RegisterRowWire<'a> {
            fn from(r: &'a RegisterRow) -> Self {
                RegisterRowWire {
                    acc_name: &r.acc_name,
                    total: &r.total,
                    running_total: &r.running_total,
                }
            }
        }
    }

    fn print_tty<'a>(
        mut out: impl Write,
        reg: impl Iterator<Item = RegisterGroup<'a>>,
    ) -> io::Result<()> {
        let mut table = Table::new();
        table.load_preset(presets::NOTHING).set_header(
            [
                "xact-id",
                "Date",
                "Payee",
                "Account",
                "Amount",
                "RunningTotal",
            ]
            .map(|s| {
                Cell::new(s)
                    .add_attribute(Attribute::Bold)
                    .set_alignment(CellAlignment::Center)
            }),
        );

        fn add_row_1(
            table: &mut Table,
            id: usize,
            date: NaiveDate,
            payee: &str,
            entry: &RegisterRow,
        ) {
            table.add_row(vec![
                Cell::new(id).set_alignment(CellAlignment::Right),
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

            add_row_1(&mut table, r.id, *r.date, r.payee, row);
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

pub mod info {
    use std::io::{self, Write};

    use super::*;
    use crate::info::JnlInfo;

    pub fn print(mut out: impl Write, report: &JnlInfo, fmt: Fmt) -> io::Result<()> {
        match fmt {
            Fmt::Json => {
                let doc = wire::InfoReport::from(report);
                writeln!(out, "{}", serde_json::to_string(&doc).unwrap())
            }
            Fmt::Lisp => {
                let doc = wire::InfoReport::from(report);
                writeln!(out, "{}", serde_lexpr::to_string(&doc).unwrap())
            }
            Fmt::Tty => print_tty(out, report),
        }
    }

    /// Stable JSON/Lisp shape for the `info` report.
    pub mod wire {
        use schemars::JsonSchema;
        use serde::Serialize;

        use crate::info::JnlInfo;
        use crate::journal::AccName;
        use crate::symbol::Symbol;

        /// Top-level shape of the `info --fmt json` report.
        ///
        /// Catalog of what exists in the (filtered) journal. On an empty
        /// journal the report is `{"accounts": [], "commodities": [],
        /// "payees": []}` and the exit code is `0`.
        ///
        /// Stability: the shape is tied to the `ledger` binary version
        /// (no independent schema versioning). Pin your tooling against a
        /// known `ledger` version; breaking changes ride with binary
        /// releases.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "InfoReport")]
        pub struct InfoReport<'a> {
            /// All account names referenced by any posting, sorted
            /// lexicographically and deduplicated.
            pub accounts: &'a [AccName],
            /// All commodity symbols referenced by any posting,
            /// sorted and deduped.
            pub commodities: &'a [Symbol],
            /// All payee strings referenced by any transaction,
            /// sorted and deduped.
            pub payees: &'a [String],
        }

        impl<'a> From<&'a JnlInfo> for InfoReport<'a> {
            fn from(j: &'a JnlInfo) -> Self {
                InfoReport {
                    accounts: &j.accounts,
                    commodities: &j.commodities,
                    payees: &j.payees,
                }
            }
        }
    }

    fn print_tty(mut out: impl Write, report: &JnlInfo) -> io::Result<()> {
        writeln!(out, "Accounts:")?;
        for acc in &report.accounts {
            writeln!(out, "  {acc}")?;
        }
        writeln!(out, "")?;
        writeln!(out, "Commodities:")?;
        for c in &report.commodities {
            writeln!(out, "  {c}")?;
        }
        writeln!(out, "")?;
        writeln!(out, "Payees:")?;
        for p in &report.payees {
            writeln!(out, "  {p}")?;
        }
        Ok(())
    }
}

pub mod print {
    use std::io::{self, Write};

    use super::*;
    use crate::journal::{Posting, State, Xact};

    /// Column at which posting amounts are aligned in the TTY output.
    const AMOUNT_COL: usize = 48;

    pub fn print<'a>(
        mut out: impl Write,
        xacts: impl Iterator<Item = &'a Xact>,
        fmt: Fmt,
    ) -> io::Result<()> {
        match fmt {
            Fmt::Tty => print_tty(out, xacts),
            Fmt::Json => {
                let doc = wire::PrintReport::from_xacts(xacts);
                writeln!(out, "{}", serde_json::to_string(&doc).unwrap())
            }
            Fmt::Lisp => {
                let doc = wire::PrintReport::from_xacts(xacts);
                writeln!(out, "{}", serde_lexpr::to_string(&doc).unwrap())
            }
        }
    }

    fn print_tty<'a>(mut out: impl Write, xacts: impl Iterator<Item = &'a Xact>) -> io::Result<()> {
        let mut first = true;
        for x in xacts {
            if !first {
                writeln!(out)?;
            }
            first = false;
            write_xact(&mut out, x)?;
        }
        Ok(())
    }

    fn write_xact(out: &mut impl Write, x: &Xact) -> io::Result<()> {
        write!(out, "{}", x.date.txdate)?;
        if let Some(ef) = x.date.efdate {
            write!(out, "={}", ef)?;
        }
        match x.state {
            State::Cleared => write!(out, " *")?,
            State::Pending => write!(out, " !")?,
            State::None => {}
        }
        if !x.code.is_empty() {
            write!(out, " ({})", x.code)?;
        }
        if !x.payee.is_empty() {
            write!(out, " {}", x.payee)?;
        }
        if !x.comment.is_empty() {
            write!(out, "  ; {}", x.comment)?;
        }
        writeln!(out)?;

        for p in &x.postings {
            write_posting(out, p)?;
        }
        Ok(())
    }

    fn write_posting(out: &mut impl Write, p: &Posting) -> io::Result<()> {
        let name = p.acc_name.to_string();
        // The "    " prefix is four spaces (ledger requires postings
        // indented). Pad so the amount starts at AMOUNT_COL.
        let head_len = 4 + name.len();
        let pad = AMOUNT_COL.saturating_sub(head_len).max(2);
        write!(out, "    {}{}{}", name, " ".repeat(pad), p.quantity)?;

        // Emit `@ uprice` only when the unit price introduces a new
        // commodity (e.g. quantity is `10 AAPL` and uprice is in `$`),
        // since the parser fills uprice with `1 quantity.s` otherwise.
        if p.uprice.s != p.quantity.s {
            write!(out, " @ {}", p.uprice)?;
        }

        // Emit `{lot_uprice}` when it carries information not already
        // expressed by `uprice`.
        if p.lot_uprice.price != p.uprice {
            write!(out, " {{{}}}", p.lot_uprice.price)?;
        }

        if let Some(ld) = p.lot_date {
            write!(out, " [{}]", ld)?;
        }
        if !p.lot_note.is_empty() {
            write!(out, " ({})", p.lot_note)?;
        }
        if !p.comment.is_empty() {
            write!(out, "  ; {}", p.comment)?;
        }
        writeln!(out)
    }

    /// Stable JSON/Lisp shape for the `print` report.
    ///
    /// Wire types decoupled from the internal `Xact`/`Posting`/`State`
    /// layout. Plain structs with `derive(Serialize, JsonSchema)` so the
    /// JSON schema is produced from the same source of truth as the
    /// output.
    pub mod wire {
        use chrono::NaiveDate;
        use schemars::JsonSchema;
        use serde::Serialize;

        use crate::journal::{AccName, Xact};
        use crate::quantity::Quantity;

        /// Top-level shape of the `print --fmt json` report.
        ///
        /// A sequence of transactions in the order returned by the query,
        /// each rendered as a [`XactWire`]. When the query matches nothing
        /// (empty journal, no filter match, or `--id N` with no such
        /// transaction) the report is `[]` and the exit code is `0`.
        #[derive(Serialize, JsonSchema)]
        #[serde(transparent)]
        #[schemars(rename = "PrintReport")]
        pub struct PrintReport<'a>(pub Vec<XactWire<'a>>);

        impl<'a> PrintReport<'a> {
            /// Build a `PrintReport` by converting each `Xact` to its wire shape.
            pub fn from_xacts<I: Iterator<Item = &'a Xact>>(xacts: I) -> Self {
                PrintReport(xacts.map(XactWire::from).collect())
            }
        }

        /// A single transaction in the report.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "Xact")]
        pub struct XactWire<'a> {
            /// Transaction date (the `txdate`).
            pub date: NaiveDate,
            /// Optional effective date — when the transaction takes accounting
            /// effect, if it differs from `date`.
            pub efdate: Option<NaiveDate>,
            /// Clearance state of the transaction.
            pub state: StateWire,
            /// Optional transaction code (e.g. check number), empty when absent.
            pub code: &'a str,
            /// Free-form counterparty / payee description.
            pub payee: &'a str,
            /// Free-form transaction-level comment, empty when absent.
            pub comment: &'a str,
            /// Postings in the order they appear in the journal.
            pub postings: Vec<PostingWire<'a>>,
            /// Transaction-level tags as flat strings (e.g. `tag` or `key:value`).
            pub tags: Vec<String>,
        }

        impl<'a> From<&'a Xact> for XactWire<'a> {
            fn from(x: &'a Xact) -> Self {
                XactWire {
                    date: x.date.txdate,
                    efdate: x.date.efdate,
                    state: x.state.into(),
                    code: &x.code,
                    payee: &x.payee,
                    comment: &x.comment,
                    postings: x.postings.iter().map(PostingWire::from).collect(),
                    tags: x.tags.iter().map(|t| t.to_string()).collect(),
                }
            }
        }

        /// A single posting line of a transaction.
        #[derive(Serialize, JsonSchema)]
        #[schemars(rename = "Posting")]
        pub struct PostingWire<'a> {
            /// Fully-qualified account name (colon-separated path).
            pub account: &'a AccName,
            /// Clearance state **as declared on this posting line** (`*`,
            /// `!`, or none). This is *not* the effective state — no
            /// inheritance from the transaction is applied. To resolve
            /// the effective state, fall back to the transaction's
            /// `state` when this field is `none`.
            pub state: StateWire,
            /// Signed amount posted to the account, expressed in its native commodity.
            pub quantity: Quantity,
            /// Unit market price (`@ price` in the journal). Always present.
            pub uprice: Quantity,
            /// Unit lot price (`{lot_price}` in the journal). Always
            /// present; tracks cost basis for lots of an investment
            /// commodity. Parser defaults: when only one of `@`/`{}` is
            /// written, the other is copied; when neither is given, both
            /// default to one unit of the quantity's own commodity.
            pub lot_uprice: Quantity,
            /// Optional lot acquisition date (`[YYYY-MM-DD]` in the journal).
            pub lot_date: Option<NaiveDate>,
            /// Optional free-form lot label (the parenthesised note).
            pub lot_note: &'a str,
            /// Free-form posting comment, empty when absent.
            pub comment: &'a str,
            /// Posting-level tags as flat strings.
            pub tags: Vec<String>,
        }

        impl<'a> From<&'a crate::journal::Posting> for PostingWire<'a> {
            fn from(p: &'a crate::journal::Posting) -> Self {
                PostingWire {
                    account: &p.acc_name,
                    state: p.state.into(),
                    quantity: p.quantity,
                    uprice: p.uprice,
                    lot_uprice: p.lot_uprice.price,
                    lot_date: p.lot_date,
                    lot_note: &p.lot_note,
                    comment: &p.comment,
                    tags: p.tags.iter().map(|t| t.to_string()).collect(),
                }
            }
        }

        /// Clearance state of a transaction or posting.
        #[derive(Serialize, JsonSchema, Clone, Copy)]
        #[serde(rename_all = "lowercase")]
        #[schemars(rename = "State")]
        pub enum StateWire {
            /// No marker (default).
            None,
            /// `*` — fully reconciled.
            Cleared,
            /// `!` — tentative / pending reconciliation.
            Pending,
        }

        impl From<crate::journal::State> for StateWire {
            fn from(s: crate::journal::State) -> Self {
                match s {
                    crate::journal::State::None => StateWire::None,
                    crate::journal::State::Cleared => StateWire::Cleared,
                    crate::journal::State::Pending => StateWire::Pending,
                }
            }
        }
    }
}
