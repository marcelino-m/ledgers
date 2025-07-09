use crate::commodity::Quantity;
use crate::parser;
use chrono::NaiveDate;
use std::io;

#[derive(Debug, Default)]
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

#[derive(Debug, Default)]
pub struct Posting {
    // posting state
    pub state: State,
    // name of the account
    pub account: String,
    //Debits and credits correspond to positive and negative values,
    // respectively.
    pub quantity: Quantity,
    // cost by unit
    pub ucost: Quantity,
    // lots
    pub lots_price: Option<Quantity>,
    pub lots_date: Option<NaiveDate>,
    pub lots_note: Option<String>,

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
        println!("xxxxxx el xact es: {:?}", xact);
    }

    return Ok(());
}
