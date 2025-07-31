use crate::commodity::Quantity;
use crate::parser;
use crate::prices::PriceType;
use chrono::NaiveDate;
use std::io;

#[derive(Debug, Default, Copy, Clone)]
pub enum State {
    #[default]
    None, // not * or !
    Cleared, // *
    Pending, // !
}

#[derive(Debug, Default)]
pub struct Xact {
    pub state: State,
    pub code: Option<String>,
    pub date: XactDate,
    pub payee: String,
    pub comment: Option<String>,
    pub postings: Vec<Posting>,
}

#[derive(Debug, Default)]
pub struct XactDate {
    pub txdate: NaiveDate,
    pub efdate: Option<NaiveDate>,
}

#[derive(Debug, Clone, Copy)]
pub struct LotPrice {
    pub price: Quantity,
    pub ptype: PriceType,
}

#[derive(Debug)]
pub struct Posting {
    // posting state
    pub state: State,
    // name of the account
    pub account: String,
    // Debits and credits correspond to positive and negative values,
    // respectively. All posting must have a quantity
    pub quantity: Quantity,
    // price by unit, if None, mean that this quantity was used as
    // primary commodity or for exchanged
    pub uprice: Option<Quantity>,
    // lot price,  if None  mean the same like uprice
    pub lot_price: Option<LotPrice>,
    // lot date
    pub lot_date: Option<NaiveDate>,
    // lot note
    pub lot_note: Option<String>,
    // posting comment
    pub comment: Option<String>,
}

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Parser(parser::ParserError),
}

impl Posting {
    /// compute the value in terms of cost of the posting
    pub fn value(&self) -> Quantity {
        self.lot_price.map(|lp| lp.price * self.quantity).unwrap()
    }
}

pub fn read_journal(r: &mut impl io::Read) -> Result<(), JournalError> {
    let mut content = String::new();

    if let Err(err) = r.read_to_string(&mut content) {
        return Err(JournalError::Io(err));
    }

    let journal = match parser::parse_journal(&content) {
        Ok(journal) => journal,
        Err(err) => return Err(JournalError::Parser(err)),
    };

    for xact in journal {
        println!("xxxxxx el xact es: {:#?}", xact);
    }

    return Ok(());
}
