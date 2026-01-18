use chrono::NaiveDate;
use regex::Regex;

use std::collections::BTreeMap;

use crate::{
    account::{AccPostingSrc, Account},
    balance_view::{BalanceView, FlatAccountView, HierAccountView},
    commodity::Valuation,
    journal::{AccName, Xact},
    ledger::Ledger,
    misc::today,
    pricedb::PriceDB,
    tamount::TAmount,
};

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
    pub fn balance(&self, v: Valuation, price_db: &PriceDB) -> TAmount {
        self.balance_as_of(today(), v, price_db)
    }

    /// Returns the total balance only considering postings up to the
    /// given date.
    pub fn balance_as_of(&self, date: NaiveDate, v: Valuation, price_db: &PriceDB) -> TAmount {
        self.accounts()
            .map(|a| a.balance_as_of(date, v, price_db))
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
    pub fn to_balance_view_as_of(
        &self,
        date: NaiveDate,
        v: Valuation,
        price_db: &PriceDB,
    ) -> BalanceView<HierAccountView<TAmount>> {
        self.accounts().fold(BalanceView::new(), |mut balv, acc| {
            let hier = acc.to_hier_view_as_of(date, v, price_db);
            balv += hier;
            balv
        })
    }

    /// Returns a hierarchical balance view of all accounts at the
    /// given dates.
    pub fn to_balance_view_at_dates(
        &self,
        v: Valuation,
        price_db: &PriceDB,
        at: impl Iterator<Item = NaiveDate>,
    ) -> BalanceView<HierAccountView<TAmount>> {
        at.fold(
            BalanceView::<FlatAccountView<TAmount>>::new(),
            |mut acc, date| {
                acc += self.to_balance_view_as_of(date, v, price_db).to_flat();
                acc
            },
        )
        .to_hier()
    }
}
