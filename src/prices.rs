use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDateTime;

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
                    .map(|p| (p.quantity.s, x.date.txdate, p.uprice))
            })
            .for_each(|(s, at, price)| {
                db.upsert_price(s, misc::to_datetime(&at), price);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::misc;
    use crate::quantity;
    use chrono::NaiveDate;
    use pretty_assertions::assert_eq;
    use rust_decimal::dec;

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
}
