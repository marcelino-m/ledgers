use std::collections::HashMap;

use chrono::NaiveDate;

use crate::{
    account::LedgerEntry,
    journal::{AccName, Journal, Xact},
};

/// The ledger contains all account
#[derive(Debug)]
pub struct Ledger<'l> {
    acounts: HashMap<&'l AccName, LedgerEntry<'l>>,
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
