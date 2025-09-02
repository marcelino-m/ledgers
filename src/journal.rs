use crate::commodity::{Quantity, Valuation};
use crate::misc;
use crate::parser;
use crate::prices::{PriceDB, PriceType};
use crate::symbol::Symbol;
use chrono::{NaiveDate, NaiveDateTime};
use std::{
    convert::From,
    fmt::{self, Debug, Display},
    io, iter,
    ops::Deref,
};

/// A market price entry in the journal i.e:
/// `P 2023-01-01 USD 1.2345 EUR`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MarketPrice {
    pub date_time: NaiveDateTime,
    pub sym: Symbol,
    pub price: Quantity,
}

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
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AccountName(String);

impl AccountName {
    /// Account name separator
    const SEP: &'static str = ":";

    /// Returns an iterator over all parent account names of this account,
    /// including the full account name itself.
    ///
    /// # Examples
    ///
    /// ```
    /// use ledger::journal::AccountName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccountName::from("Assets:Bank:Checking");
    /// let parents: Vec<&str> = acc.all_accounts().collect();
    /// assert_eq!(parents, vec!["Assets", "Assets:Bank", "Assets:Bank:Checking"]);
    /// ```
    pub fn all_accounts(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccountName::SEP)
            .map(|(i, _)| &self.0[..i])
            .chain(iter::once(&self.0[..]))
    }

    /// Like [`all_accounts`] but exclude the full account
    ///
    /// # Examples
    /// ```
    /// use ledger::journal::AccountName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccountName::from("Assets:Bank:Checking");
    /// let parents: Vec<&str> = acc.parent_accounts().collect();
    /// assert_eq!(parents, vec!["Assets", "Assets:Bank"]);
    /// ```
    pub fn parent_accounts(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccountName::SEP)
            .map(|(i, _)| &self.0[..i])
    }

    /// Return the root account of the hierarchy.
    /// # Examples
    /// ```
    /// use ledger::journal::AccountName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccountName::from("Assets:Bank:Checking");
    /// assert_eq!(acc.parent_account(), "Assets");
    /// ```
    pub fn parent_account(&self) -> &str {
        let Some(t) = self.0.find(AccountName::SEP) else {
            return &self.0;
        };

        &self.0[..t]
    }

    /// Returns an iterator over the account name parts, split by `":"`.
    ///
    /// # Examples
    /// ```
    /// use ledger::journal::AccountName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccountName::from("Assets:Bank:Checking");
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
    /// use ledger::journal::AccountName;
    /// use std::str::FromStr;
    ///
    /// let acc = AccountName::from("Assets:Bank");
    /// let acc = acc.append(&("Checking".into()));
    /// let exp = AccountName::from("Assets:Bank:Checking");
    /// assert_eq!(acc, exp);
    ///
    /// let acc = AccountName::from("");
    /// let acc = acc.append(&("Checking".into()));
    /// let exp = AccountName::from("Checking");
    /// assert_eq!(acc, exp);
    /// ```
    pub fn append(&self, sub: &AccountName) -> Self {
        if self.is_empty() {
            sub.clone()
        } else {
            AccountName(format!("{}:{}", &self, &sub))
        }
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

#[derive(Debug, PartialEq, Eq)]
pub struct Xact {
    pub state: State,
    pub code: Option<String>,
    pub date: XactDate,
    pub payee: String,
    pub comment: Option<String>,
    pub postings: Vec<Posting>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Posting {
    /// posting date, is the same date as the transaction date
    /// (Xact::date::txdate)
    pub date: NaiveDate,

    /// posting state
    pub state: State,
    /// name of the account
    pub account: AccountName,
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
    pub lot_note: Option<String>,
    /// posting comment
    pub comment: Option<String>,
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
            .price_as_of(self.quantity.s, misc::to_datetime(&self.date))
            .unwrap();
        uprice * self.quantity
    }
}

pub struct Journal {
    xact: Vec<Xact>,
    market_prices: Vec<MarketPrice>,
}

impl Journal {
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
    Parser(parser::ParserError),
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
