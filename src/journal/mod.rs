use std::{
    collections::{HashMap, HashSet},
    convert::From,
    fmt::{self, Debug, Display},
    io, iter, mem,
    ops::Deref,
};

use chrono::NaiveDate;
use serde::Serialize;

use crate::pricedb::{MarketPrice, PriceDB, PriceType};
use crate::{
    balance::AccPostingSrc,
    misc::{self, BetweenDate},
};
use crate::{
    commodity::{Quantity, Valuation},
    tags::Tag,
};

mod parser;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum State {
    None,    // It's neither * nor !
    Cleared, // *
    Pending, // !
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LotPrice {
    pub price: Quantity,
    pub ptype: PriceType,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct XactDate {
    pub txdate: NaiveDate,
    pub efdate: Option<NaiveDate>,
}

/// The name of an account.
///
/// Account names can use a colon-separated hierarchy to represent
/// account structure. For example: `"Assets:Bank:Checking"`
/// and `"Assets:Cash"`.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Default)]
pub struct AccName(String);

impl AccName {
    /// Account name separator
    const SEP: &'static str = ":";

    /// Returns an iterator over all parent account names of this account,
    /// including the full account name itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use ledger::journal::AccName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccName::from("Assets:Bank:Checking");
    /// let parents: Vec<&str> = acc.all_accounts().collect();
    /// assert_eq!(parents, vec!["Assets", "Assets:Bank", "Assets:Bank:Checking"]);
    /// ```
    pub fn all_accounts(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccName::SEP)
            .map(|(i, _)| &self.0[..i])
            .chain(iter::once(&self.0[..]))
    }

    /// Like [`all_accounts`] but exclude the full account
    ///
    /// # Examples
    /// ```
    /// use ledger::journal::AccName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccName::from("Assets:Bank:Checking");
    /// let parents: Vec<&str> = acc.parent_accounts().collect();
    /// assert_eq!(parents, vec!["Assets", "Assets:Bank"]);
    /// ```
    pub fn parent_accounts(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccName::SEP)
            .map(|(i, _)| &self.0[..i])
    }

    /// Return the root account of the hierarchy.
    /// # Examples
    /// ```
    /// use ledger::journal::AccName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccName::from("Assets:Bank:Checking");
    /// assert_eq!(acc.parent_account(), "Assets");
    /// ```
    pub fn parent_account(&self) -> &str {
        let Some(t) = self.0.find(AccName::SEP) else {
            return &self.0;
        };

        &self.0[..t]
    }

    /// Returns an iterator over the account name parts, split by `":"`.
    ///
    /// # Examples
    /// ```
    /// use ledger::journal::AccName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccName::from("Assets:Bank:Checking");
    /// let parts: Vec<&str> = acc.split_parts().collect();
    /// assert_eq!(parts, vec!["Assets", "Bank", "Checking"]);
    /// ```
    pub fn split_parts(&self) -> impl Iterator<Item = &str> {
        self.0.split(":")
    }

    /// Appends a sub-account to the current account name,
    /// joining them with `":"`.
    /// If the current name is empty, returns the sub-account directly.
    ///
    /// # Examples
    /// ```
    /// use ledger::journal::AccName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccName::from("Assets:Bank");
    /// let acc = acc.append(&("Checking".into()));
    /// let exp = AccName::from("Assets:Bank:Checking");
    /// assert_eq!(acc, exp);
    ///
    /// let acc = AccName::from("");
    /// let acc = acc.append(&("Checking".into()));
    /// let exp = AccName::from("Checking");
    /// assert_eq!(acc, exp);
    /// ```
    pub fn append(&self, sub: &AccName) -> Self {
        if self.is_empty() {
            sub.clone()
        } else {
            AccName(format!("{}:{}", &self, &sub))
        }
    }

    /// Extracts and removes the first parent account from the account name.
    ///
    /// For an account name like "parent:child:grandchild", this function:
    /// - Returns `Some(AccName("parent"))`
    /// - Updates self to "child:grandchild"
    ///
    /// Returns `None` if the account name is empty or has no parent separator.
    ///
    /// # Example
    /// ```
    /// use ledger::journal::AccName;
    ///
    /// let mut acc_name = AccName::from("assets:bank:checking".to_string());
    /// let parent = acc_name.pop_parent_account();
    /// assert_eq!(parent, Some(AccName::from("assets".to_string())));
    /// assert_eq!(acc_name, AccName::from("bank:checking".to_string()));
    /// ```
    pub fn pop_parent_account(&mut self) -> Option<AccName> {
        if self.is_empty() {
            return None;
        }

        let cnt = mem::take(&mut self.0);
        let mut it = cnt.split(AccName::SEP);
        let pop = it.next().unwrap();
        self.0 = it.collect::<Vec<_>>().join(":");

        return Some(AccName(pop.to_owned()));
    }
}

impl Deref for AccName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Debug for AccName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Display for AccName {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AccName {
    fn from(s: String) -> Self {
        AccName(s)
    }
}

impl From<&str> for AccName {
    fn from(s: &str) -> Self {
        AccName(s.to_owned())
    }
}
/// Each transaction (xact) must balance to zero.
///
/// The sum of all posting unit prices (lot_uprice) must meet one of
/// the following conditions:
/// 1. **Zero Balance:** The sum equals zero.
/// 2. **Commodity Conversion:** The sum is a simple two-commodity conversion
///    (e.g., nC1 - mC2), which implicitly defines an exchange rate. This form
///    is considered balanced because the conversion means nC1 - mC2 = 0.
///
/// If neither condition is met, the transaction is unbalanced and
/// should be flagged as an error.
///
/// **Inferring Unit Price (Conversion):** If a posting specifies a
/// `quantity` in terms of C1 or C2 but lacks a `lot_uprice` specified
/// in terms of the *other* commodity, ledger attempts to identify the
/// **primary commodity** (C1 or C2). ledger then establishes the
/// correct `lot_uprice` using the primary commodity as the valuation
/// basis. A primary commodity is always valued in terms of itself;
/// this logic applies to `uprice` as well.
#[derive(Debug, PartialEq, Eq)]
pub struct Xact {
    pub state: State,
    pub code: String,
    pub date: XactDate,
    pub payee: String,
    pub comment: String,
    pub postings: Vec<Posting>,
    /// transaction tags (e.g. `:tag:` or `:tag1:tag2:`)
    pub tags: Vec<Tag>,
    /// transaction vtags (value tags) (e.g. `tag1: some value`)
    pub vtags: HashMap<Tag, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Posting {
    /// posting date, is the same date as the transaction date
    /// (Xact::date::txdate)
    pub date: NaiveDate,

    /// posting state
    pub state: State,
    /// name of the account
    pub acc_name: AccName,
    /// Debits and credits correspond to positive and negative values,
    /// respectively
    pub quantity: Quantity,
    /// `uprice` is the unitary market price of the quantity.  This
    /// value is either provided, or it defaults to `lot_uprice` if
    /// `lot_uprice` is present.  Otherwise, it defaults to 1 in terms
    /// of the commodity itself (`quantity / quantity`).
    pub uprice: Quantity,
    /// `lot_uprice` is the unitary lot price of the quantity.  This
    /// value is either provided, or it defaults to `uprice` if
    /// `uprice` is present.  Otherwise, it defaults to 1 in terms of
    /// the commodity itself (`quantity / quantity`).
    pub lot_uprice: LotPrice,
    /// lot date
    pub lot_date: Option<NaiveDate>,
    /// lot note
    pub lot_note: String,
    /// posting comment
    pub comment: String,
    /// posting tags (e.g. `:tag:` or `:tag1:tag2:`)
    pub tags: Vec<Tag>,
    /// posting vtags (value tags) (e.g. `tag1: some value`)
    pub vtags: HashMap<Tag, String>,
}

struct AccPosting<'a> {
    acc_name: &'a AccName,
    postings: &'a [Posting],
}

impl<'a> AccPostingSrc<'a> for AccPosting<'a> {
    fn acc_name(&self) -> &AccName {
        self.acc_name
    }

    fn postings(&self) -> Box<dyn Iterator<Item = &'a Posting> + 'a> {
        Box::new(
            self.postings
                .iter()
                .filter(|p| p.acc_name == *self.acc_name),
        )
    }
}

impl<'a> Xact {
    /// Get all postings group by account
    pub fn get_all_postings(&'a self) -> impl Iterator<Item = impl AccPostingSrc<'a>> {
        let mut seen = HashSet::new();
        let distinct = self
            .postings
            .iter()
            .filter(move |p| seen.insert(&p.acc_name));

        distinct.map(|p| AccPosting {
            acc_name: &p.acc_name,
            postings: self.postings.as_slice(),
        })
    }
}

impl Posting {
    /// compute the value of the posting according to the given
    /// valuation mode
    pub fn value(&self, val: Valuation, price_db: &PriceDB) -> Quantity {
        match val {
            Valuation::Quantity => self.quantity,
            Valuation::Basis => self.book_value(),
            Valuation::Market => self.market_value(price_db),
            Valuation::Historical => self.historical_value(price_db),
        }
    }

    /// compute the value of the posting in terms of lot `{price}`
    pub fn book_value(&self) -> Quantity {
        self.lot_uprice.price * self.quantity
    }

    /// compute the market value of the posting using the latest price
    pub fn market_value(&self, price_db: &PriceDB) -> Quantity {
        let uprice = price_db.latest_price(self.quantity.s);
        uprice * self.quantity
    }

    /// Computes the value of this posting using the historical
    /// (market value as of transaction date) prices.
    pub fn historical_value(&self, price_db: &PriceDB) -> Quantity {
        let uprice = price_db
            .price_as_of(self.quantity.s, misc::to_datetime(self.date))
            .unwrap();
        uprice * self.quantity
    }
}

pub struct Journal {
    xact: Vec<Xact>,
    market_prices: Vec<MarketPrice>,
}

impl Journal {
    pub fn filter_by_date(self, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        let between = BetweenDate::new(from, to);
        let xact = self
            .xact
            .into_iter()
            .filter(|x| between.check(x.date.txdate))
            .collect::<Vec<_>>();

        let market_prices = self
            .market_prices
            .into_iter()
            .filter(|p| between.check(p.date_time.date()))
            .collect::<Vec<_>>();

        Journal {
            xact,
            market_prices,
        }
    }

    /// returns an iterator over all transactions in the journal
    pub fn xacts(&self) -> impl Iterator<Item = &Xact> {
        self.xact.iter()
    }

    /// like `xacts` but returns only the first `n` transactions
    pub fn xacts_head(&self, n: usize) -> impl Iterator<Item = &Xact> {
        self.xact.iter().take(n)
    }

    /// like `xacts` but returns only the last `n` transactions
    pub fn xacts_tail(&self, n: usize) -> impl Iterator<Item = &Xact> {
        self.xact.iter().rev().take(n).rev()
    }

    /// returns an iterator over all market prices in the journal
    pub fn market_prices(&self) -> impl Iterator<Item = &MarketPrice> {
        self.market_prices.iter()
    }
}

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Parser(parser::ParseError),
}

pub fn read_journal(mut r: impl io::Read) -> Result<Journal, JournalError> {
    let mut content = String::new();

    if let Err(err) = r.read_to_string(&mut content) {
        return Err(JournalError::Io(err));
    }

    let parsed = match parser::parse_journal(&content) {
        Ok(journal) => journal,
        Err(err) => return Err(JournalError::Parser(err)),
    };

    return Ok(Journal {
        xact: parsed.xacts,
        market_prices: parsed.market_prices,
    });
}
