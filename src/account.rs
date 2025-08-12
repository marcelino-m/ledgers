use chrono::NaiveDate;

use crate::{
    commodity::Amount,
    journal::{AccountName, Posting, Xact},
};

/// Represents a ledger account.
///
/// An `Account` stores a collection of `Entry` objects,
/// each linking a transaction (`Xact`) to its corresponding posting (`Posting`).
#[derive(Debug)]
pub struct Account<'l> {
    pub name: &'l AccountName,
    entries: Vec<Entry<'l>>,
}

/// Represents a single ledger entry within an account.
///
/// Each `Entry` references:
/// - A transaction ([`Xact`]) it belongs to.
/// - A specific posting ([`Posting`]) within that transaction.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'l> {
    pub xact: &'l Xact,
    pub posting: &'l Posting,
}

impl<'l> Account<'l> {
    /// Creates an empty account with the given name.
    pub fn from_name(name: &'l AccountName) -> Account<'l> {
        Account {
            name,
            entries: Vec::new(),
        }
    }

    /// Adds a new ledger entry to this account.
    ///
    /// An entry combines a transaction ([`Xact`]) and a posting
    /// ([`Posting`]) from that transaction.
    pub fn add_register(&mut self, xact: &'l Xact, p: &'l Posting) {
        self.entries.push(Entry {
            xact: xact,
            posting: p,
        });
    }

    /// Computes the current balance of this account.
    ///
    /// Sums the `quantity` of all postings in this account.
    pub fn balance(&self) -> Amount {
        self.entries.iter().map(|e| e.posting.quantity).sum()
    }

    /// Computes the current balance of this account in its base cost.
    ///
    /// Sums the result of `base_cost()` for all postings.
    pub fn book_balance(&self) -> Amount {
        self.entries.iter().map(|e| e.posting.book_value()).sum()
    }

    /// Filters entries in this account by a date range using the
    /// transaction date of the posting.
    pub fn filter_by_date(&self, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Account<'l> {
        let between = |date| {
            (from.is_none() || date >= from.unwrap()) && (to.is_none() || date <= to.unwrap())
        };

        let filtered = self
            .entries
            .iter()
            .filter(|&e| between(e.date()))
            .cloned()
            .collect();

        Account {
            name: self.name,
            entries: filtered,
        }
    }

    /// Returns true if the account has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns all the entries of this account
    pub fn get_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }
}

impl<'l> Entry<'l> {
    /// Returns the date of the transaction associated with this
    /// entry.
    pub fn date(&self) -> NaiveDate {
        self.xact.date.txdate
    }
}
