use regex::Regex;

use std::collections::BTreeMap;

use crate::{
    balance_view::{BalanceView, HierAccountView, utils},
    commodity::{Amount, Valuation},
    journal::{AccName, Posting, Xact},
    ledger::Ledger,
    pricedb::PriceDB,
};

/// Provides access to postings for a specific account.
pub trait AccPostingSrc<'a> {
    fn acc_name(&self) -> &AccName;
    fn postings(&self) -> Box<dyn Iterator<Item = &'a Posting> + 'a>;
}

/// An `Account` acts as a container for the set of all debits and
/// credits (postings) made to this specific account across various
/// transactions.
///
/// The account balance can be calculated using different `Valuation``
/// schemes: `Basis`, `Quantity`, or `Market` or `Historical`.
pub struct Account<'a> {
    /// the full name
    name: AccName,
    postings: Box<dyn AccPostingSrc<'a> + 'a>,
}

/// Represents a collection of accounts.
#[derive(Default)]
pub struct Balance<'a> {
    accnts: BTreeMap<AccName, Account<'a>>,
}

impl<'a> Account<'a> {
    /// Creates a new account from the given name and postings.
    pub fn from_postings(name: AccName, ps: impl AccPostingSrc<'a> + 'a) -> Account<'a> {
        Account {
            name,
            postings: Box::new(ps),
        }
    }

    /// Returns the name of this account.
    pub fn name(&self) -> &AccName {
        &self.name
    }

    /// Returns the balance of the account
    pub fn balance(&self, v: Valuation, price_db: &PriceDB) -> Amount {
        self.postings.postings().map(|p| p.value(v, price_db)).sum()
    }

    /// Converts this account into its full hierarchical representation.
    ///
    /// This method expands the account into a tree structure (`HierAccount`),
    /// where each component of the account name becomes a nested subaccount.
    ///
    /// For example, an account with the name `Assets:Bank:Checking $300` would be
    ///   Assets   $300
    ///    |- Bank     $300
    ///        |- Checking  $300
    ///
    /// The resulting structure preserves the complete hierarchy and balance
    /// information of the original account.
    pub fn to_hier(&self, v: Valuation, price_db: &PriceDB) -> HierAccountView {
        let name = self.name().clone();
        let bal = self.balance(v, price_db);
        utils::build_hier_account(name, bal).unwrap()
    }
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
