use chrono::NaiveDate;

use crate::{
    balance_view::{HierAccountView, utils},
    commodity::{Amount, Valuation},
    journal::{AccName, Posting},
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
        self.balance_as_of(NaiveDate::MAX, v, price_db)
    }

    /// Like `balance` but only considering postings up to and including
    /// the given date.
    pub fn balance_as_of(&self, date: NaiveDate, v: Valuation, price_db: &PriceDB) -> Amount {
        self.postings
            .postings()
            .filter(|p| p.date <= date)
            .map(|p| p.value(v, price_db))
            .sum()
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
    pub fn to_hier_view(&self, v: Valuation, price_db: &PriceDB) -> HierAccountView {
        self.to_hier_view_as_of(NaiveDate::MAX, v, price_db)
    }

    /// Like `to_hier_view` but only considering postings up to and
    /// including date
    pub fn to_hier_view_as_of(
        &self,
        date: NaiveDate,
        v: Valuation,
        price_db: &PriceDB,
    ) -> HierAccountView {
        let name = self.name().clone();
        let bal = self.balance_as_of(date, v, price_db);
        utils::build_hier_account(name, bal).unwrap()
    }
}
