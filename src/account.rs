use chrono::NaiveDate;

use crate::{
    commodity::Amount,
    journal::{AccName, Posting, Xact},
    pricedb::PriceDB,
};

/// An association between an account and all the transactions and
/// postings in which it appears.
#[derive(Debug)]
pub struct LedgerEntry<'l> {
    pub acc_name: &'l AccName,
    entries: Vec<XactPosting<'l>>,
}

/// A link beetween a posting and the transaction where it appears
#[derive(Debug, Clone, Copy)]
pub struct XactPosting<'l> {
    pub xact: &'l Xact,
    pub posting: &'l Posting,
}

impl<'l> LedgerEntry<'l> {
    /// Creates an empty account with the given name.
    pub fn from_name(name: &'l AccName) -> LedgerEntry<'l> {
        LedgerEntry {
            acc_name: name,
            entries: Vec::new(),
        }
    }

    /// Adds a new ledger entry to this account.
    ///
    /// An entry combines a transaction ([`Xact`]) and a posting
    /// ([`Posting`]) from that transaction.
    pub fn add_register(&mut self, xact: &'l Xact, p: &'l Posting) {
        self.entries.push(XactPosting {
            xact: xact,
            posting: p,
        });
    }

    /// Computes the current balance of this account bu summing all
    /// commodities in this account.
    pub fn balance(&self) -> Amount {
        self.entries.iter().map(|e| e.posting.quantity).sum()
    }

    /// Computes the current balance of this account using the
    /// original book cost.
    pub fn book_balance(&self) -> Amount {
        self.entries.iter().map(|e| e.posting.book_value()).sum()
    }

    /// Computes the current market value of this account.
    pub fn market_balance(&self, price_db: &PriceDB) -> Amount {
        self.balance()
            .iter_quantities()
            .map(|qty| price_db.latest_price(qty.s) * qty.q)
            .sum()
    }

    /// Computes the balance of this account using the historical
    /// (market value as of transaction date) prices.
    pub fn historical_value(&self, price_db: &PriceDB) -> Amount {
        self.entries
            .iter()
            .map(|e| e.posting.historical_value(price_db))
            .sum()
    }

    /// Filters entries in this account by a date range using the
    /// transaction date of the posting.
    pub fn filter_by_date(
        &self,
        from: Option<NaiveDate>,
        to: Option<NaiveDate>,
    ) -> LedgerEntry<'l> {
        let between = |date| {
            (from.is_none() || date >= from.unwrap()) && (to.is_none() || date <= to.unwrap())
        };

        let filtered = self
            .entries
            .iter()
            .filter(|&e| between(e.date()))
            .cloned()
            .collect();

        LedgerEntry {
            acc_name: self.acc_name,
            entries: filtered,
        }
    }

    /// Returns true if the account has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns all the entries of this account
    pub fn get_entries(&self) -> impl Iterator<Item = &XactPosting> {
        self.entries.iter()
    }
}

impl<'l> XactPosting<'l> {
    /// Returns the date of the transaction associated with this
    /// entry.
    pub fn date(&self) -> NaiveDate {
        self.xact.date.txdate
    }
}
