use std::collections::BTreeMap;
use std::iter::Sum;

use chrono::NaiveDate;
use regex::Regex;

use crate::{
    account::{AccPostingSrc, Account},
    account_view::{FlatAccountView, HierAccountView},
    balance_view::BalanceView,
    holdings::Lot,
    journal::{AccName, Xact},
    ledger::Ledger,

    ntypes::{Arithmetic, Basket, Valuable},
    pricedb::PriceDB,
    tamount::TAmount,
};

/// Specifies the method to calculate the commodity price
/// value.
///
/// # Variants
///
/// - `Basis`: Calculate using the book value
/// - `Quantity`: Calculate based on raw quantities without valuation.
/// - `Market`: Calculate using the most recent market value from the price database.
#[derive(Debug, Copy, Clone)]
pub enum Valuation {
    Basis,
    Quantity,
    Market,
    Historical,
}

/// Represents a collection of accounts.
#[derive(Default)]
pub struct Balance<'a> {
    accnts: BTreeMap<AccName, Account<'a>>,
}

impl<'a> Balance<'a> {
    /// Creates a new, empty balance.
    ///
    /// The balance is initialized with no accounts and a flat layout.
    pub fn new() -> Balance<'a> {
        Self::default()
    }

    /// Creates a new balance from the given ledger and optional regex
    pub fn from_ledger<'b>(ledger: &'b Ledger, qry: &[Regex]) -> Balance<'b> {
        Balance {
            accnts: ledger
                .get_all_posting()
                .filter(|ps| qry.is_empty() || qry.iter().any(|r| r.is_match(ps.acc_name())))
                .map(|ps| {
                    (
                        ps.acc_name().clone(),
                        Account::from_postings(ps.acc_name().clone(), ps),
                    )
                })
                .collect(),
        }
    }

    /// Creates a new balance from the given transaction.
    pub fn from_xact<'b>(xact: &'b Xact) -> Balance<'b> {
        Balance {
            accnts: xact
                .get_all_postings()
                .map(|ps| {
                    (
                        ps.acc_name().clone(),
                        Account::from_postings(ps.acc_name().clone(), ps),
                    )
                })
                .collect(),
        }
    }

    /// Returns the total balance of all accounts.
    pub fn balance<V>(&self, price_db: &PriceDB) -> V
    where
        V: Arithmetic + Basket + Valuable + Sum<Lot>,
    {
        self.balance_as_of(NaiveDate::MAX, price_db)
    }

    /// Returns the total balance only considering postings up to the
    /// given date.
    pub fn balance_as_of<V>(&self, date: NaiveDate, price_db: &PriceDB) -> V
    where
        V: Arithmetic + Basket + Valuable + Sum<Lot>,
    {
        self.accounts()
            .map(|a| a.balance_as_of::<V>(date, price_db))
            .sum()
    }

    /// Returns an iterator over all accounts as immutable references.
    pub fn accounts(&self) -> impl Iterator<Item = &Account<'a>> {
        self.accnts.values()
    }

    /// Consumes the balance and returns an iterator over its accounts.
    pub fn into_accounts(self) -> impl Iterator<Item = Account<'a>> {
        self.accnts.into_values()
    }

    /// Returns a hierarchical balance view of all accounts as of the
    /// given date.
    pub fn to_balance_view_as_of<V>(
        &self,
        date: NaiveDate,
        price_db: &PriceDB,
    ) -> BalanceView<HierAccountView<TAmount<V>>>
    where
        V: Arithmetic + Basket + Valuable + Sum<Lot>,
    {
        self.accounts().fold(BalanceView::new(), |mut balv, acc| {
            let hier = acc.to_hier_view_as_of(date, price_db);
            balv += hier;
            balv
        })
    }

    /// Returns a hierarchical balance view of all accounts at the
    /// given dates.
    pub fn to_balance_view_at_dates<V>(
        &self,
        price_db: &PriceDB,
        at: impl Iterator<Item = NaiveDate>,
    ) -> BalanceView<HierAccountView<TAmount<V>>>
    where
        V: Basket + Arithmetic + Valuable + Sum<Lot>,
    {
        at.fold(
            BalanceView::<FlatAccountView<TAmount<V>>>::new(),
            |mut acc, date| {
                acc += self.to_balance_view_as_of(date, price_db).to_flat();
                acc
            },
        )
        .to_hier()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::holdings::Holdings;
    use crate::ntypes::TsBasket;
    use crate::ntypes::Zero;
    use crate::{misc, util};

    #[test]
    fn test_balance() {
        let input = "\
2026-02-19 transaction 1
  A             $1
  A:B           $-1
";
        let (journal, price_db) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();
        let ledger = Ledger::from_journal(&journal);

        let bal = Balance::from_ledger(&ledger, &[]);
        let total = bal.balance::<Holdings>(&price_db);
        assert!(total.is_zero());

        let total = bal.balance_as_of::<Holdings>(misc::today(), &price_db);
        assert!(total.is_zero());

        let balv = bal.to_balance_view_as_of::<Holdings>(misc::today(), &price_db);
        let total = balv.balance().at(misc::today()).cloned().unwrap();
        assert!(total.is_zero());

        let flat = balv.to_flat();
        let total = flat.balance().at(misc::today()).cloned().unwrap();
        assert!(total.is_zero());
    }
}
