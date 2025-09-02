use std::collections::{BTreeMap, HashMap};
use std::io;

use chrono::NaiveDateTime;

use crate::journal::MarketPrice;
use crate::parser::{self, ParserError};
use crate::{commodity::Quantity, journal::Journal, misc, symbol::Symbol};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PriceType {
    Static,
    Floating,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PriceBasis {
    PerUnit,
    Total,
}

/// A simple in-memory database for storing prices of commodities over
/// time.
pub struct PriceDB {
    data: HashMap<Symbol, BTreeMap<NaiveDateTime, Quantity>>,
}

impl PriceDB {
    /// Creates a new, empty `PriceDB`.
    pub fn new() -> PriceDB {
        PriceDB {
            data: HashMap::new(),
        }
    }

    /// Constructs a `PriceDB` from a `Journal`
    pub fn from_journal(journal: &Journal) -> PriceDB {
        let mut db = PriceDB::new();
        journal
            .xacts()
            .flat_map(|x| {
                x.postings
                    .iter()
                    .map(|p| (p.quantity.s, misc::to_datetime(&x.date.txdate), p.uprice))
            })
            .chain(
                journal
                    .market_prices()
                    .map(|mp| (mp.sym, mp.date_time, mp.price)),
            )
            .for_each(|(s, at, price)| {
                db.upsert_price(s, at, price);
            });
        db
    }

    /// Updates or inserts the price for a given commodity on a
    /// specific date.
    pub fn upsert_price(&mut self, s: Symbol, at: NaiveDateTime, price: Quantity) {
        self.data
            .entry(s)
            .or_insert(BTreeMap::new())
            .insert(at, price);
    }

    /// Retrieves the most recent price of a symbol. All symbols
    /// always have a latest price, in the worst case it's the book
    /// value
    pub fn latest_price(&self, s: Symbol) -> Quantity {
        self.data
            .get(&s)
            .and_then(|prices| prices.values().next_back().copied())
            .unwrap()
    }

    /// Retrieves the most recent price of a symbol up to a given
    /// date.
    pub fn price_as_of(&self, s: Symbol, at: NaiveDateTime) -> Option<Quantity> {
        self.data
            .get(&s)
            .and_then(|prices| prices.range(..=at).next_back().map(|(_, &price)| price))
    }
}

/// Parses a price database from a buffered reader, returning an
/// iterator over the parsed prices or errors. An error don't stop the
/// parsing, all lines are processed.
pub fn read_price_db(
    bread: impl io::BufRead,
) -> impl Iterator<Item = Result<MarketPrice, ParserError>> {
    bread.lines().map(|line| match line {
        Ok(line) => match parser::parse_market_price_line(&line) {
            Ok(price) => return Ok(price),
            Err(err) => return Err(err),
        },
        Err(err) => Err(ParserError::IOErr(err)),
    })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use pretty_assertions::assert_eq;
    use rust_decimal::dec;

    use super::*;
    use crate::misc;
    use crate::quantity;

    #[test]
    fn test_price_db() {
        let mut db = PriceDB::new();
        let s1 = Symbol::new("USD");
        let s2 = Symbol::new("$");
        let at1 = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let at2 = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        let at3 = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();

        let at1 = misc::to_datetime(&at1);
        let at2 = misc::to_datetime(&at2);
        let at3 = misc::to_datetime(&at3);

        db.upsert_price(s1, at1, quantity!(100.0, "USD"));
        db.upsert_price(s1, at2, quantity!(105.0, "USD"));
        db.upsert_price(s1, at3, quantity!(110.0, "USD"));
        db.upsert_price(s2, at1, quantity!(1.0, "$"));
        db.upsert_price(s2, at2, quantity!(1.05, "$"));

        assert_eq!(db.latest_price(s1), quantity!(110.0, "USD"));
        assert_eq!(db.price_as_of(s1, at2), Some(quantity!(105.0, "USD")));
        let t = NaiveDate::from_ymd_opt(2022, 12, 31).unwrap();
        assert_eq!(db.price_as_of(s1, misc::to_datetime(&t)), None);
        assert_eq!(db.latest_price(s2), quantity!(1.05, "$"));
        assert_eq!(db.price_as_of(s2, at1), Some(quantity!(1.0, "$")));
    }

    #[test]
    fn test_from_journal() {
        let jf = "\
P 2025/07/25 LTM  $ 20.15
P 2025/08/09  12:00:00 LTM $ 21.10

2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     -10 LTM {{$300.00}} [2025/08/29] @@ $200.00
    * Assets:Cash

P 2025/08/28 LTM  $ 23.69
";

        let journal = crate::journal::read_journal(jf.as_bytes()).unwrap();
        let db = PriceDB::from_journal(&journal);

        let s = Symbol::new("LTM");
        let d1 = misc::to_datetime(&NaiveDate::from_ymd_opt(2025, 7, 25).unwrap());
        let d2 = misc::to_datetime(&NaiveDate::from_ymd_opt(2025, 8, 9).unwrap());
        let d3 = misc::to_datetime(&NaiveDate::from_ymd_opt(2025, 8, 28).unwrap());
        let d4 = misc::to_datetime(&NaiveDate::from_ymd_opt(2025, 5, 11).unwrap());

        assert_eq!(db.latest_price(s), quantity!(23.69, "$"));
        assert_eq!(db.price_as_of(s, d1), Some(quantity!(20.15, "$")));
        assert_eq!(db.price_as_of(s, d2), Some(quantity!(20.15, "$")));
        assert_eq!(db.price_as_of(s, d3), Some(quantity!(23.69, "$")));
        assert_eq!(db.price_as_of(s, d4), Some(quantity!(20.00, "$")));
    }
}
