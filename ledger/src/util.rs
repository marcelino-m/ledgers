use crate::journal::{self, Journal, JrnIO};
use crate::pricedb::{self, PriceDB};
use std::io::BufRead;

#[derive(Debug)]
pub enum ReadDbError {
    JournalError(journal::JournalError),
    PriceDBError(pricedb::ParseError),
}

use pricedb::ReadItem;

/// Reads a journal and builds the companion `PriceDB` holding the
/// market prices used for valuation.
///
/// The resulting `PriceDB` is the single source of truth for market
/// prices on any given date. Its entries come from three places, in
/// order of precedence as they are inserted:
///
/// 1. **Posting unit prices**: every posting in the journal contributes
///    the unit price implied by its quantity and amount (e.g.
///    `10 AAPL @ $150` records AAPL at $150 on the transaction date).
/// 2. **`P` directives in the journal**: explicit
///    `P DATE SYM PRICE` lines override or supplement posting-derived
///    prices for that date.
/// 3. **External price database** (the optional `pricedb` argument):
///    additional `P`-style entries from a separate file, applied last
///    and therefore overriding any earlier value at the same
///    `(symbol, date)` key.
///
/// In short: for a given commodity on a given date, the market price
/// is whatever the journal's postings recorded, unless a `P` entry —
/// in the journal itself or in the price-db file — supplies one.
pub fn read_journal_and_price_db(
    journal: JrnIO,
    pricedb: Option<Box<dyn BufRead>>,
) -> Result<(journal::Journal, pricedb::PriceDB), ReadDbError> {
    let journal = match Journal::new(journal) {
        Ok(journal) => journal,
        Err(err) => {
            return Err(ReadDbError::JournalError(err));
        }
    };

    let mut price_db = PriceDB::from_journal(&journal);
    let Some(reader) = pricedb else {
        return Ok((journal, price_db));
    };

    pricedb::read_price_db(reader).for_each(|item| match item {
        ReadItem::Price(p) => price_db.upsert_price(p.sym, p.date_time, p.price),
        ReadItem::ParseError(e) => {
            eprintln!("Error parsing price db line: {:?}", e);
        }
        ReadItem::IoError(e) => {
            eprint!("Error reading price db file: {:?}", e);
        }
    });

    Ok((journal, price_db))
}
