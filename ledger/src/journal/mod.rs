use std::{
    collections::{HashMap, HashSet},
    convert::From,
    fmt::{self, Debug, Display},
    io, iter, mem,
    ops::Deref,
};

use chrono::NaiveDate;
use serde::Serialize;

use crate::{
    account::AccPostingSrc,
    amount::Amount,
    balance::Valuation,
    misc::BetweenDate,
    pricedb::{MarketPrice, PriceDB, PriceType},
    quantity::Quantity,
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
    /// assert_eq!(acc.parent_account(), Some("Assets"));
    /// ```
    pub fn parent_account(&self) -> Option<&str> {
        let Some(t) = self.0.find(AccName::SEP) else {
            return None;
        };

        Some(&self.0[..t])
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

        Some(AccName(pop.to_owned()))
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
    /// compute the value of the posting in terms of lot `{price}`
    pub fn book_value(&self) -> Quantity {
        self.lot_uprice.price * self.quantity.q
    }
}

pub struct Journal {
    xact: Vec<Xact>,
    market_prices: Vec<MarketPrice>,
}

impl Journal {
    /// Filters the journal to include only transactions and market
    /// prices within the specified date range.
    pub fn filter_by_date(mut self, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        let between = BetweenDate::new(from, to);
        self.xact.retain(|x| between.check(x.date.txdate));
        self.market_prices
            .retain(|p| between.check(p.date_time.date()));
        self
    }
    /// returns the total number of transactions in the journal
    pub fn nxact(&self) -> usize {
        self.xact.len()
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

    Ok(Journal {
        xact: parsed.xacts,
        market_prices: parsed.market_prices,
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::quantity;
    use crate::util;
    use chrono::NaiveDate;
    use rust_decimal::dec;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn all_accounts_three_levels() {
        let acc = AccName::from("A:B:C");
        let all: Vec<&str> = acc.all_accounts().collect();
        assert_eq!(all, vec!["A", "A:B", "A:B:C"]);
    }

    #[test]
    fn all_accounts_single_component() {
        let acc = AccName::from("Root");
        let all: Vec<&str> = acc.all_accounts().collect();
        assert_eq!(all, vec!["Root"]);
    }

    #[test]
    fn all_accounts_two_levels() {
        let acc = AccName::from("Assets:Bank");
        let all: Vec<&str> = acc.all_accounts().collect();
        assert_eq!(all, vec!["Assets", "Assets:Bank"]);
    }

    // --- AccName::parent_accounts ---

    #[test]
    fn parent_accounts_three_levels() {
        let acc = AccName::from("A:B:C");
        let parents: Vec<&str> = acc.parent_accounts().collect();
        assert_eq!(parents, vec!["A", "A:B"]);
    }

    #[test]
    fn parent_accounts_single_component_is_empty() {
        let acc = AccName::from("Root");
        let parents: Vec<&str> = acc.parent_accounts().collect();
        assert!(parents.is_empty());
    }

    #[test]
    fn parent_accounts_two_levels() {
        let acc = AccName::from("Assets:Bank");
        let parents: Vec<&str> = acc.parent_accounts().collect();
        assert_eq!(parents, vec!["Assets"]);
    }

    #[test]
    fn parent_account_returns_root() {
        let acc = AccName::from("Assets:Bank:Checking");
        assert_eq!(acc.parent_account(), Some("Assets"));
    }

    #[test]
    fn parent_account_single_component_returns_self() {
        let acc = AccName::from("Expenses");
        let p = acc.parent_account();
        assert!(p.is_none());
    }

    #[test]
    fn split_parts_three_levels() {
        let acc = AccName::from("Assets:Bank:Checking");
        let parts: Vec<&str> = acc.split_parts().collect();
        assert_eq!(parts, vec!["Assets", "Bank", "Checking"]);
    }

    #[test]
    fn split_parts_single_component() {
        let acc = AccName::from("Root");
        let parts: Vec<&str> = acc.split_parts().collect();
        assert_eq!(parts, vec!["Root"]);
    }

    #[test]
    fn append_to_existing() {
        let acc = AccName::from("Assets:Bank");
        let sub = AccName::from("Checking");
        let result = acc.append(&sub);
        assert_eq!(result, AccName::from("Assets:Bank:Checking"));
    }

    #[test]
    fn append_to_empty() {
        let acc = AccName::from("");
        let sub = AccName::from("Checking");
        let result = acc.append(&sub);
        assert_eq!(result, AccName::from("Checking"));
    }

    #[test]
    fn append_multi_level_sub() {
        let acc = AccName::from("A");
        let sub = AccName::from("B:C");
        let result = acc.append(&sub);
        assert_eq!(result, AccName::from("A:B:C"));
    }

    #[test]
    fn filter_by_date_from_only() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B

2025-03-01 third
  A          $300
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let filtered = journal.filter_by_date(Some(d(2025, 2, 1)), None);
        let xacts: Vec<&str> = filtered.xacts().map(|x| x.payee.as_str()).collect();
        assert_eq!(xacts, vec!["second", "third"]);
    }

    #[test]
    fn filter_by_date_to_only() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B

2025-03-01 third
  A          $300
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let filtered = journal.filter_by_date(None, Some(d(2025, 2, 1)));
        let xacts: Vec<&str> = filtered.xacts().map(|x| x.payee.as_str()).collect();
        assert_eq!(xacts, vec!["first", "second"]);
    }

    #[test]
    fn filter_by_date_from_and_to() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B

2025-03-01 third
  A          $300
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let filtered = journal.filter_by_date(Some(d(2025, 1, 15)), Some(d(2025, 2, 15)));
        let xacts: Vec<&str> = filtered.xacts().map(|x| x.payee.as_str()).collect();
        assert_eq!(xacts, vec!["second"]);
    }

    #[test]
    fn filter_by_date_none_none_returns_all() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let filtered = journal.filter_by_date(None, None);
        let xacts: Vec<&str> = filtered.xacts().map(|x| x.payee.as_str()).collect();
        assert_eq!(xacts, vec!["first", "second"]);
    }

    #[test]
    fn filter_by_date_filters_market_prices_too() {
        let input = "\
P 2025/01/01 LTM $ 10.00
P 2025/03/01 LTM $ 20.00
P 2025/05/01 LTM $ 30.00

2025-01-01 dummy
  A          $100
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let filtered = journal.filter_by_date(Some(d(2025, 2, 1)), Some(d(2025, 4, 1)));
        let prices: Vec<_> = filtered.market_prices().collect();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].price, quantity!(20.00, "$"));
    }

    #[test]
    fn xacts_head_returns_first_n() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B

2025-03-01 third
  A          $300
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let head: Vec<&str> = journal.xacts_head(2).map(|x| x.payee.as_str()).collect();
        assert_eq!(head, vec!["first", "second"]);
    }

    #[test]
    fn xacts_head_more_than_total_returns_all() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let head: Vec<&str> = journal.xacts_head(10).map(|x| x.payee.as_str()).collect();
        assert_eq!(head, vec!["first", "second"]);
    }

    #[test]
    fn xacts_head_zero_returns_none() {
        let input = "\
2025-01-01 first
  A          $100
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let head: Vec<&str> = journal.xacts_head(0).map(|x| x.payee.as_str()).collect();
        assert!(head.is_empty());
    }

    #[test]
    fn xacts_tail_returns_last_n() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B

2025-03-01 third
  A          $300
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let tail: Vec<&str> = journal.xacts_tail(2).map(|x| x.payee.as_str()).collect();
        assert_eq!(tail, vec!["second", "third"]);
    }

    #[test]
    fn xacts_tail_more_than_total_returns_all() {
        let input = "\
2025-01-01 first
  A          $100
  B

2025-02-01 second
  A          $200
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let tail: Vec<&str> = journal.xacts_tail(10).map(|x| x.payee.as_str()).collect();
        assert_eq!(tail, vec!["first", "second"]);
    }

    #[test]
    fn xacts_tail_zero_returns_none() {
        let input = "\
2025-01-01 first
  A          $100
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let tail: Vec<&str> = journal.xacts_tail(0).map(|x| x.payee.as_str()).collect();
        assert!(tail.is_empty());
    }

    #[test]
    fn xacts_tail_preserves_order() {
        let input = "\
2025-01-01 alpha
  A          $1
  B

2025-02-01 beta
  A          $2
  B

2025-03-01 gamma
  A          $3
  B

2025-04-01 delta
  A          $4
  B
";
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(input.as_bytes()), None).unwrap();

        let tail: Vec<&str> = journal.xacts_tail(3).map(|x| x.payee.as_str()).collect();
        assert_eq!(tail, vec!["beta", "gamma", "delta"]);
    }

    #[test]
    fn read_journal_io_error_returns_err() {
        // An implementation of Read that always fails to trigger JournalError::Io
        struct FailReader;
        impl std::io::Read for FailReader {
            fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "forced IO error",
                ))
            }
        }
        let result = read_journal(FailReader);
        assert!(matches!(result, Err(JournalError::Io(_))));
    }
}
