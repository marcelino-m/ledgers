use chrono::NaiveDate;

use crate::{
    commodity::Amount,
    journal::{Posting, Xact},
};

use std::{
    convert::From,
    fmt::{self, Debug, Display},
    ops::Deref,
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

/// The name of an account.
///
/// Account names can use a colon-separated hierarchy to represent
/// account structure. For example: `"Assets:Bank:Checking"`
/// and `"Assets:Cash"`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountName(String);

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

impl AccountName {
    /// Account name separator
    const SEP: &'static str = ":";

    /// Returns an iterator over all parent account names of this account,
    /// including the full account name itself.
    ///
    /// # Examples
    ///
    /// ```
    /// let acc = AccountName::from_str("Assets:Bank:Checking".to_string());
    /// let parents: Vec<&str> = acc.parents().collect();
    /// assert_eq!(parents, vec!["Assets", "Assets:Bank"]);
    /// ```
    pub fn all_accounts(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccountName::SEP)
            .map(|(i, _)| &self.0[..=i])
    }

    /// Like [`all_accounts`] but exclude the full account
    pub fn parent_accounts(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccountName::SEP)
            .map(|(i, _)| &self.0[..i])
    }

    /// Return the root account of the hierarchy.
    pub fn parent_account(&self) -> AccountName {
        let Some(t) = self.0.find(AccountName::SEP) else {
            return AccountName::from_str(self.0.clone());
        };

        AccountName::from_str(self.0[..t].to_owned())
    }
}

impl Deref for AccountName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Debug for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for AccountName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AccountName {
    fn from(s: String) -> Self {
        AccountName(s)
    }
}

impl From<&str> for AccountName {
    fn from(s: &str) -> Self {
        AccountName(s.to_owned())
    }
}
