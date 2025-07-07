use std::io;

use crate::parser;

#[derive(Debug)]
pub enum State {
    Cleared, // *
    Pending, // !
    None,    // not * or !
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
