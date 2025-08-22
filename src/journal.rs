use crate::commodity::Quantity;
use crate::parser;
use crate::prices::PriceType;
use crate::{account::AccountName, prices::PriceDB};

use chrono::NaiveDate;

use std::{fmt::Debug, io};

pub struct Journal {
    pub xact: Vec<Xact>,
}

pub fn read_journal(mut r: impl io::Read) -> Result<Journal, JournalError> {
    let mut content = String::new();

    if let Err(err) = r.read_to_string(&mut content) {
        return Err(JournalError::Io(err));
    }

    let xacts = match parser::parse_journal(&content) {
        Ok(journal) => journal,
        Err(err) => return Err(JournalError::Parser(err)),
    };

    return Ok(Journal { xact: xacts });
}

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

#[derive(Debug, Default, PartialEq, Eq)]
pub struct XactDate {
    pub txdate: NaiveDate,
    pub efdate: Option<NaiveDate>,
}

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
        let uprice = price_db.price_as_of(self.quantity.s, self.date).unwrap();
        uprice * self.quantity
    }
}
