use std::collections::HashMap;

use chrono::NaiveDate;

use crate::account::AccPostingSrc;
use crate::journal::{AccName, Journal, Posting, Xact};
use crate::misc::BetweenDate;

#[derive(Debug)]
pub struct Ledger<'l> {
    acc_posting: HashMap<&'l AccName, Vec<&'l Posting>>,
}

struct AccPosting<'a> {
    acc_name: AccName,
    postings: &'a Vec<&'a Posting>,
}

impl<'a> AccPostingSrc<'a> for AccPosting<'a> {
    fn acc_name(&self) -> &AccName {
        &self.acc_name
    }

    fn postings(&self) -> Box<dyn Iterator<Item = &'a Posting> + 'a> {
        Box::new(self.postings.iter().copied())
    }
}

impl<'l> Ledger<'l> {
    /// Creates a new [`Ledger`] from a list of transactions [`Xact`].
    pub fn from_journal(journal: &'l Journal) -> Ledger<'l> {
        let mut ledger = Ledger {
            acc_posting: HashMap::new(),
        };

        ledger.fill_from_xacts(journal.xacts());
        ledger
    }

    /// Returns a new [`Ledger`] containing only the accounts with transactions
    /// whose dates fall within the optional `from` and `to` date range.
    pub fn filter_by_date(&self, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        let between = BetweenDate::new(from, to);

        let acc = self
            .acc_posting
            .iter()
            .map(|(&name, postings)| {
                (
                    name,
                    postings
                        .iter()
                        .filter(|&e| between.check(e.date))
                        .copied()
                        .collect(),
                )
            })
            .collect();

        Ledger { acc_posting: acc }
    }

    /// Returns an immutable reference to a ledger entry of an account
    /// by name.
    pub fn get_acc_postings<'a>(&'a self, name: &AccName) -> Option<impl AccPostingSrc<'a>> {
        self.acc_posting.get(name).map(|ps| AccPosting {
            acc_name: name.clone(),
            postings: ps,
        })
    }

    /// Returns an iterator over all accounts in the ledger.
    pub fn get_all_posting<'a>(&'a self) -> impl Iterator<Item = impl AccPostingSrc<'a>> {
        self.acc_posting.iter().map(|(&acc_name, ps)| AccPosting {
            acc_name: acc_name.clone(),
            postings: ps,
        })
    }

    /// Returns a mutable reference to a ledger entry of an account
    /// by name.
    fn get_entry_mut(&mut self, name: &'l AccName) -> &mut Vec<&'l Posting> {
        self.acc_posting.entry(name).or_default()
    }

    /// Populates the ledger by iterating over all transactions and
    /// postings and each posting is registered in the corresponding
    /// account.
    fn fill_from_xacts(&mut self, xacts: impl Iterator<Item = &'l Xact>) -> &mut Self {
        for xact in xacts {
            for p in &xact.postings {
                let acc = self.get_entry_mut(&p.acc_name);
                // TODO: optimize this: try to determine needed space
                acc.push(p);
            }
        }
        self
    }
}
