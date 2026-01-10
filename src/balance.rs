use regex::Regex;

use std::collections::BTreeMap;

use crate::{
    account::{AccPostingSrc, Account},
    balance_view::{BalanceView, HierAccountView},
    commodity::{Amount, Valuation},
    journal::{AccName, Xact},
    ledger::Ledger,
    pricedb::PriceDB,
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
    pub fn balance(&self, v: Valuation, price_db: &PriceDB) -> Amount {
        self.accounts().map(|a| a.balance(v, price_db)).sum()
    }

    /// Returns an iterator over all accounts as immutable references.
    pub fn accounts(&self) -> impl Iterator<Item = &Account<'a>> {
        self.accnts.values()
    }

    /// Consumes the balance and returns an iterator over its accounts.
    pub fn into_accounts(self) -> impl Iterator<Item = Account<'a>> {
        self.accnts.into_values()
    }

    pub fn to_balance_view(
        &self,
        v: Valuation,
        price_db: &PriceDB,
    ) -> BalanceView<HierAccountView> {
        self.accounts().fold(BalanceView::new(), |mut balv, acc| {
            let hier = acc.to_hier(v, price_db);
            balv += hier;
            balv
        })
    }
}
