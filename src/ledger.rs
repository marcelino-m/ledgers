use std::collections::HashMap;

use chrono::NaiveDate;

use crate::journal::{AccName, Journal, Posting, Xact};
use crate::{commodity::Amount, pricedb::PriceDB};

/// The ledger contains all account
#[derive(Debug)]
pub struct Ledger<'l> {
    acounts: HashMap<&'l AccName, LedgerEntry<'l>>,
}

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

impl<'l> XactPosting<'l> {
    /// Returns the date of the transaction associated with this
    /// entry.
    pub fn date(&self) -> NaiveDate {
        self.xact.date.txdate
    }
}

impl<'l> Ledger<'l> {
    /// Creates a new [`Ledger`] from a list of transactions [`Xact`].
    pub fn from_journal(journal: &'l Journal) -> Ledger<'l> {
        let mut ledger = Ledger {
            acounts: HashMap::new(),
        };

        ledger.fill_from_xacts(journal.xacts());
        ledger
    }

    /// Returns a new [`Ledger`] containing only the accounts with transactions
    /// whose dates fall within the optional `from` and `to` date range.
    pub fn filter_by_date(&self, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        let acc = self
            .acounts
            .iter()
            .filter_map(|(&name, acc)| {
                let acc = acc.filter_by_date(from, to);
                if acc.is_empty() {
                    None
                } else {
                    Some((name, acc))
                }
            })
            .collect();

        Ledger { acounts: acc }
    }

    /// Returns an immutable reference to a ledger entry of an account
    /// by name.
    pub fn get_entry(&self, name: &'l AccName) -> Option<&LedgerEntry<'l>> {
        self.acounts.get(name)
    }

    /// Returns an iterator over all accounts in the ledger.
    pub fn get_entries(&self) -> impl Iterator<Item = &LedgerEntry<'l>> {
        self.acounts.values()
    }

    /// Returns a mutable reference to a ledger entry of an account
    /// by name.
    fn get_entry_mut(&mut self, name: &'l AccName) -> &mut LedgerEntry<'l> {
        self.acounts
            .entry(name)
            .or_insert(LedgerEntry::from_name(name))
    }

    /// Populates the ledger by iterating over all transactions and
    /// postings and each posting is registered in the corresponding
    /// account.
    fn fill_from_xacts(&mut self, xacts: impl Iterator<Item = &'l Xact>) -> &mut Self {
        for xact in xacts {
            for p in &xact.postings {
                let acc = self.get_entry_mut(&p.acc_name);
                acc.add_register(xact, p)
            }
        }
        self
    }
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
