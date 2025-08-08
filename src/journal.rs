use crate::commodity::Quantity;
use crate::parser;
use crate::prices::PriceType;
use chrono::NaiveDate;
use std::io;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum State {
    None,    // It's neither * nor !
    Cleared, // *
    Pending, // !
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AccountName(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LotPrice {
    pub price: Quantity,
    pub ptype: PriceType,
}

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Parser(parser::ParserError),
}

impl Posting {
    /// compute the value in terms of cost of the posting
    pub fn base_cost(&self) -> Quantity {
        self.lot_uprice.price * self.quantity
    }
}

impl AccountName {
    /// Account name separator
    const SEP: &'static str = ":";

    /// Creates a new `AccountName` from an account name string.
    pub fn from_str(name: String) -> AccountName {
        AccountName(name)
    }

    /// Returns an iterator over all parent account names of this account,
    /// excluding the full account name itself.
    ///
    /// # Examples
    ///
    /// ```
    /// let acc = AccountName::from_str("Assets:Bank:Checking".to_string());
    /// let parents: Vec<&str> = acc.parents().collect();
    /// assert_eq!(parents, vec!["Assets", "Assets:Bank"]);
    /// ```
    pub fn parents(&self) -> impl Iterator<Item = &str> {
        self.0
            .match_indices(AccountName::SEP)
            .map(|(i, _)| &self.0[..i])
    }
}

type Jounrnal = Vec<Xact>;

pub fn read_journal(mut r: impl io::Read) -> Result<Jounrnal, JournalError> {
    let mut content = String::new();

    if let Err(err) = r.read_to_string(&mut content) {
        return Err(JournalError::Io(err));
    }

    let journal = match parser::parse_journal(&content) {
        Ok(journal) => journal,
        Err(err) => return Err(JournalError::Parser(err)),
    };

    return Ok(journal);
}
