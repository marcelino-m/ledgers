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

#[derive(Debug)]
pub struct LotPrice {
    pub price: Quantity,
    pub pbasis: PriceType,
}

#[derive(Debug)]
pub struct Posting {
    // posting state
    pub state: State,
    // name of the account
    pub account: String,
    //Debits and credits correspond to positive and negative values,
    // respectively.
    // This have sense only when quantity is made up only of simple
    // Amount (one Quantity type)
    // lots
    pub quantity: Quantity,
    pub uprice: Option<Quantity>,
    pub lot_price: Option<LotPrice>,
    pub lot_date: Option<NaiveDate>,
    pub lot_note: Option<String>,
    // posting comment
    pub comment: Option<String>,
}

#[derive(Debug)]
pub enum JournalError {
    Io(io::Error),
    Parser(parser::ParserError),
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
