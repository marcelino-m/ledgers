use std::iter::Sum;

use chrono::NaiveDate;

use crate::{
    account_view,
    account_view::HierAccountView,
    holdings::Lot,
    journal::{AccName, Posting},
    misc::{to_datetime, today},
    ntypes::{Arithmetic, Basket, Valuable},
    pricedb::PriceDB,
    tamount::TAmount,
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
    pub fn balance<V>(&self, price_db: &PriceDB) -> V
    where
        V: Basket + Arithmetic + Valuable + Sum<Lot>,
    {
        self.balance_as_of(today(), price_db)
    }

    /// Like `balance` but only considering postings up to and including
    /// the given date.
    pub fn balance_as_of<V>(&self, date: NaiveDate, price_db: &PriceDB) -> V
    where
        V: Basket + Arithmetic + Valuable + Sum<Lot>,
    {
        self.postings
            .postings()
            .filter(|p| p.date <= date)
            .map(|p| {
                let b = p.lot_uprice.price;
                let m = price_db
                    .price_as_of(p.quantity.s, to_datetime(date))
                    .unwrap();
                let h = price_db
                    .price_as_of(p.quantity.s, to_datetime(p.date))
                    .unwrap();

                Lot {
                    qty: p.quantity,
                    m_uprice: m.to_amount(),
                    h_uprice: h.to_amount(),
                    b_uprice: b.to_amount(),
                }
            })
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
    pub fn to_hier_view<V>(&self, price_db: &PriceDB) -> HierAccountView<TAmount<V>>
    where
        V: Arithmetic + Basket + Valuable + Sum<Lot>,
    {
        self.to_hier_view_as_of(today(), price_db)
    }

    /// Like `to_hier_view` but only considering postings up to and
    /// including date
    pub fn to_hier_view_as_of<V>(
        &self,
        date: NaiveDate,
        price_db: &PriceDB,
    ) -> HierAccountView<TAmount<V>>
    where
        V: Arithmetic + Basket + Valuable + Sum<Lot>,
    {
        let name = self.name().clone();
        let bal = self.balance_as_of(date, price_db);
        let bal = [(date, bal)].into_iter().collect();

        account_view::utils::build_hier_account(name, bal).unwrap()
    }
}
