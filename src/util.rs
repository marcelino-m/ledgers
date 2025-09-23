use crate::journal;
use crate::pricedb::{self, PriceDB};
use std::fs::File;
use std::io;

#[derive(Debug)]
pub enum ReadDbError {
    JournalError(journal::JournalError),
    PriceDBError(pricedb::ParseError),
    IOErr(io::Error),
}

use pricedb::ReadItem;

pub fn read_journal_and_price_db(
    journal_path: String,
    pricedb_path: Option<String>,
) -> Result<(journal::Journal, pricedb::PriceDB), ReadDbError> {
    let file = match File::open(&journal_path) {
        Ok(file) => file,
        Err(err) => {
            return Err(ReadDbError::JournalError(journal::JournalError::Io(err)));
        }
    };

    let journal = match journal::read_journal(file) {
        Ok(journal) => journal,
        Err(err) => {
            return Err(ReadDbError::JournalError(err));
        }
    };

    let mut price_db = PriceDB::from_journal(&journal);
    let Some(path) = pricedb_path else {
        return Ok((journal, price_db));
    };

    match pricedb::read_price_db_file(path) {
        Ok(iter) => {
            iter.for_each(|item| match item {
                ReadItem::Price(p) => price_db.upsert_price(p.sym, p.date_time, p.price),
                ReadItem::ParseError(e) => {
                    eprintln!("Error parsing price db line: {:?}", e);
                }
                ReadItem::IoError(e) => {
                    eprint!("Error reading price db file: {:?}", e);
                }
            });
        }
        Err(err) => return Err(ReadDbError::IOErr(err)),
    }

    Ok((journal, price_db))
}
