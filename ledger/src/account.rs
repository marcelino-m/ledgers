use std::iter::Sum;

use chrono::NaiveDate;

use crate::{
    account_view,
    account_view::HierAccountView,
    holdings::Lot,
    journal::{AccName, Posting},
    misc::to_datetime,
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
        self.balance_as_of(NaiveDate::MAX, price_db)
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
        self.to_hier_view_as_of(NaiveDate::MAX, price_db)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account_view::AccountView;
    use crate::balance;
    use crate::holdings::{Holdings, Lot};
    use crate::journal;
    use crate::ledger;
    use crate::pricedb;
    use crate::quantity;
    use crate::util;
    use chrono::NaiveDate;
    use rust_decimal::dec;
    use std::io::Cursor;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn build_journal(input: &str) -> (journal::Journal, pricedb::PriceDB) {
        let bytes = input.to_owned().into_bytes();
        util::read_journal_and_price_db(Box::new(Cursor::new(bytes)), None).unwrap()
    }

    #[test]
    fn account_name_returns_correct_name() {
        let input = "\
2026-01-01 test
  Assets:Cash    $100
  Income        $-100
";
        let (journal, price_db) = build_journal(input);
        let ledger = ledger::Ledger::from_journal(&journal);
        let bal = balance::Balance::from_ledger(&ledger, &[]);

        let cash_name = AccName::from("Assets:Cash");
        let account = bal.account(&cash_name).expect("Assets:Cash account not found");

        // line 46: name() returns the account name
        assert_eq!(account.name(), &cash_name);
        let _ = price_db;
        let _ = d(2026, 1, 1);
    }

    #[test]
    fn account_balance_returns_sum_of_postings() {
        let input = "\
2026-01-01 first
  Assets:Cash    $100
  Income        $-100

2026-02-01 second
  Assets:Cash    $50
  Income        $-50
";
        let (journal, price_db) = build_journal(input);
        let ledger = ledger::Ledger::from_journal(&journal);
        let bal = balance::Balance::from_ledger(&ledger, &[]);

        let cash_name = AccName::from("Assets:Cash");
        let account = bal.account(&cash_name).expect("Assets:Cash account not found");

        let total = account.balance::<Holdings>(&price_db);
        let uprice = quantity!(1, "$").to_amount();
        assert_eq!(
            total,
            Holdings::from_lots([Lot {
                qty: quantity!(150, "$"),
                m_uprice: uprice.clone(),
                h_uprice: uprice.clone(),
                b_uprice: uprice,
            }])
        );
    }

    #[test]
    fn account_to_hier_view_uses_today() {
        let input = "\
2026-01-01 test
  Assets:Cash    $200
  Income        $-200
";
        let (journal, price_db) = build_journal(input);
        let ledger = ledger::Ledger::from_journal(&journal);
        let bal = balance::Balance::from_ledger(&ledger, &[]);

        let cash_name = AccName::from("Assets:Cash");
        let account = bal.account(&cash_name).expect("Assets:Cash not found");

        // line 101: to_hier_view() calls to_hier_view_as_of with today()
        let hier = account.to_hier_view::<Holdings>(&price_db);
        // The root of the hierarchy should be "Assets"
        assert_eq!(hier.name(), &AccName::from("Assets"));
    }
}
