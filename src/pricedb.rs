use std::collections::{BTreeMap, HashMap};

use crate::{commodity::Quantity, journal::Journal, misc, symbol::Symbol};
use chrono::NaiveDateTime;

pub use parser::{parse_market_price_line, ParseError, ParseResult};

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

/// A market price entry in the journal i.e:
/// `P 2023-01-01 USD 1.2345 EUR`
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MarketPrice {
    pub date_time: NaiveDateTime,
    pub sym: Symbol,
    pub price: Quantity,
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
                    .map(|p| (p.quantity.s, misc::to_datetime(x.date.txdate), p.uprice))
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

mod parser {
    use std::str::FromStr;

    use crate::iter::MultiPeek;
    use atoi::atoi;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
    use rust_decimal::Decimal;

    use crate::commodity::Quantity;
    use crate::pricedb::MarketPrice;
    use crate::symbol::Symbol;

    #[derive(Debug)]
    pub enum ParseError {
        ExpectedP,
        ExpectedDate,
        ExpectedTimeOrSymbol,
        UnexpectedEndOfInput,
        ExpectedSymbol,
        EndQuoteOfSymbolNotFound,
        ExpectedPrice,
        ExpectedNum,
        InvalidDate,
        InvalidTime,
    }

    pub type ParseResult<T> = Result<T, ParseError>;

    /// Parses a market price str into a `MarketPrice` structure.
    ///
    /// # Format
    /// The input line should have the format:
    /// `P DATE[ TIME] SYM PRICE` or
    /// `P DATE[ TIME] PRICE SYM`
    pub fn parse_market_price_line(line: &str) -> ParseResult<MarketPrice> {
        let mut iter = MultiPeek::new(line.chars());

        let c = iter.next().unwrap();
        if c != 'P' {
            return Err(ParseError::ExpectedP);
        } else {
            discard_ws(&mut iter, false);
        }

        let dt = read_date_time(&mut iter)?;
        let sym = read_sym(&mut iter)?;
        let price = read_commodity(&mut iter)?;

        return Ok(MarketPrice {
            date_time: dt,
            sym: Symbol::new(&sym),
            price: price,
        });
    }

    fn read_date_time<I>(input: &mut MultiPeek<I>) -> ParseResult<NaiveDateTime>
    where
        I: Iterator<Item = char>,
    {
        let Some(try_date) = read_until(input, |c| c.is_whitespace()) else {
            return Err(ParseError::ExpectedDate);
        };

        let Ok(year) = try_date[0..4].parse::<i32>() else {
            return Err(ParseError::ExpectedDate);
        };
        let Ok(month) = try_date[5..7].parse::<u32>() else {
            return Err(ParseError::ExpectedDate);
        };
        let Ok(day) = try_date[8..10].parse::<u32>() else {
            return Err(ParseError::ExpectedDate);
        };

        let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
            return Err(ParseError::InvalidDate);
        };

        let Some(try_time) = peek_next_word(input) else {
            return Err(ParseError::UnexpectedEndOfInput);
        };

        let try_time = try_time.as_bytes();
        let time = if try_time.len() == 8 && try_time[2] == b':' {
            // looks like a time
            let Some(hour) = atoi::<u32>(&try_time[0..2]) else {
                return Err(ParseError::ExpectedTimeOrSymbol);
            };
            let Some(min) = atoi::<u32>(&try_time[3..5]) else {
                return Err(ParseError::ExpectedTimeOrSymbol);
            };
            let Some(sec) = atoi::<u32>(&try_time[6..8]) else {
                return Err(ParseError::ExpectedTimeOrSymbol);
            };

            let Some(time) = NaiveTime::from_hms_opt(hour, min, sec) else {
                return Err(ParseError::InvalidTime);
            };
            input.consume_peeked();
            time
        } else {
            NaiveTime::from_hms_opt(0, 0, 0).unwrap()
        };

        Ok(NaiveDateTime::new(date, time))
    }

    fn read_sym<I>(input: &mut MultiPeek<I>) -> ParseResult<String>
    where
        I: Iterator<Item = char>,
    {
        let allow = |c: &char| {
            !matches!(
                c,
                '0'..='9'
                    | ' '
                    | '\t'
                    | '.'
                    | ','
                    | ';'
                    | ':'
                    | '?'
                    | '!'
                    | '-'
                    | '+'
                    | '*'
                    | '/'
                    | '^'
                    | '&'
                    | '|'
                    | '='
                    | '{'
                    | '}'
                    | '['
                    | ']'
                    | '<'
                    | '>'
                    | '('
                    | ')'
                    | '@'
            )
        };

        discard_ws(input, false);

        let mut r = String::new();
        if input.peek() == Some(&&'"') {
            r.push(input.next().unwrap());
            loop {
                match input.next() {
                    Some(c) => {
                        r.push(c);
                        if c == '"' {
                            return Ok(r);
                        }
                        continue;
                    }
                    None => {
                        return Err(ParseError::EndQuoteOfSymbolNotFound);
                    }
                }
            }
        } else {
            input.unpeek();
        }

        loop {
            match input.peek() {
                Some(c) => {
                    if allow(&c) {
                        r.push(input.next().unwrap());
                        continue;
                    } else {
                        input.unpeek();
                        break;
                    }
                }
                None => {
                    break;
                }
            }
        }

        if r.is_empty() {
            return Err(ParseError::ExpectedSymbol);
        }

        Ok(r)
    }

    fn read_commodity<I>(input: &mut MultiPeek<I>) -> ParseResult<Quantity>
    where
        I: Iterator<Item = char>,
    {
        discard_ws(input, false);

        let c = match input.peek() {
            Some(&c) => c,
            None => return Err(ParseError::ExpectedPrice),
        };

        input.unpeek();

        if matches!(c, '0'..='9' | '+' | '-') {
            let mut q = String::new();
            loop {
                match input.peek() {
                    Some(c) => {
                        if matches!(c, '0'..='9' | '.' | ',' | '+' | '-') {
                            q.push(input.next().unwrap());
                            continue;
                        } else {
                            input.unpeek();
                            break;
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
            if q.is_empty() {
                return Err(ParseError::ExpectedNum);
            }

            // TODO: [DECIMAL]
            q = q.replace(",", "");
            let Ok(q) = Decimal::from_str(&q) else {
                return Err(ParseError::ExpectedNum);
            };
            let s = read_sym(input)?;
            let s = Symbol::new(&s);
            return Ok(Quantity { q, s });
        }

        // SYM NUM
        let s = read_sym(input)?;
        let s = Symbol::new(&s);

        let q = read_until(input, |c| c.is_whitespace())
            .unwrap()
            .replace(",", "");
        let Ok(q) = Decimal::from_str(&q) else {
            return Err(ParseError::ExpectedNum);
        };

        return Ok(Quantity { q, s });
    }

    fn read_until<I, F>(input: &mut MultiPeek<I>, until: F) -> Option<String>
    where
        I: Iterator<Item = char>,
        F: Fn(&char) -> bool,
    {
        input.peek_reset();
        discard_ws(input, false);

        let mut r = String::new();
        loop {
            match input.peek() {
                Some(c) => {
                    if until(c) {
                        input.unpeek();
                        break;
                    }
                    r.push(input.next().unwrap());
                    continue;
                }
                None => {
                    break;
                }
            }
        }

        if r.is_empty() {
            return None;
        }
        return Some(r);
    }

    fn peek_next_word<I>(input: &mut MultiPeek<I>) -> Option<String>
    where
        I: Iterator<Item = char>,
    {
        discard_ws(input, true);

        let mut r = String::new();
        loop {
            match input.peek() {
                Some(c) => {
                    if c.is_whitespace() {
                        input.unpeek();
                        break;
                    }
                    r.push(*c);
                    continue;
                }
                None => {
                    break;
                }
            }
        }

        if r.is_empty() {
            return None;
        }
        return Some(r);
    }

    fn discard_ws<I>(input: &mut MultiPeek<I>, only_peek: bool) -> usize
    where
        I: Iterator<Item = char>,
    {
        if !only_peek {
            // only reset if the fn is used for peeking only
            input.peek_reset();
        }

        let mut ndiscard = 0;
        loop {
            match input.peek() {
                Some(c) => {
                    if c.is_whitespace() {
                        ndiscard += 1;
                        if only_peek {
                            continue;
                        }
                        input.next();
                        continue;
                    }
                    input.unpeek();
                    break;
                }
                _ => {
                    break;
                }
            }
        }

        ndiscard
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        use crate::quantity;
        use rust_decimal::dec;
        #[test]
        fn test_market_price_parse() {
            let line = "P 2025/09/13 AAPL $ 150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));

            let line = "P 2025/09/13 12:13:14 AAPL $ 150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));
            assert_eq!(
                mp.date_time,
                NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(2025, 9, 13).unwrap(),
                    NaiveTime::from_hms_opt(12, 13, 14).unwrap()
                )
            );

            let line = "P 2025/09/13 AAPL $150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));

            let line = "P 2025/09/13 AAPL \"any-cmdty\" 150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "\"any-cmdty\""));
        }

        #[test]
        fn test_market_price_parse_reverse() {
            let line = "P 2025/09/13 AAPL  150.25 $";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));

            let line = "P 2025/09/13 12:13:14 AAPL  150.25 $";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));
            assert_eq!(
                mp.date_time,
                NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(2025, 9, 13).unwrap(),
                    NaiveTime::from_hms_opt(12, 13, 14).unwrap()
                )
            );

            let line = "P 2025/09/13 AAPL 150.25$";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));

            let line = "P 2025/09/13 AAPL  150.25 \"any-cmdty\"";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "\"any-cmdty\""));
        }

        #[test]
        fn test_market_price_with_spaces_parse() {
            let line = "P    2025/09/13    AAPL $   150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));
            let line = "P 2025/09/13          12:13:14 AAPL   $ 150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));
            assert_eq!(
                mp.date_time,
                NaiveDateTime::new(
                    NaiveDate::from_ymd_opt(2025, 9, 13).unwrap(),
                    NaiveTime::from_hms_opt(12, 13, 14).unwrap()
                )
            );
            let line = "P 2025/09/13 AAPL           $150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "$"));

            let line = "P 2025/09/13         AAPL \"any-cmdty\" 150.25";
            let mp = parse_market_price_line(line).expect("Failed to parse market price");
            assert_eq!(mp.sym, Symbol::new("AAPL"));
            assert_eq!(mp.price, quantity!(150.25, "\"any-cmdty\""));
        }

        #[test]
        fn test_read_commodity() {
            let mut qty = MultiPeek::new("$ 123.00".chars());
            let qty = read_commodity(&mut qty).unwrap();
            assert_eq!(
                qty,
                Quantity {
                    q: dec!(123.00),
                    s: Symbol::new("$")
                }
            );

            let mut qty = MultiPeek::new("$123.00".chars());
            let qty = read_commodity(&mut qty).unwrap();
            assert_eq!(
                qty,
                Quantity {
                    q: dec!(123.00),
                    s: Symbol::new("$")
                }
            );

            let mut qty = MultiPeek::new("123.00XYZ".chars());
            let qty = read_commodity(&mut qty).unwrap();
            assert_eq!(
                qty,
                Quantity {
                    q: dec!(123.00),
                    s: Symbol::new("XYZ")
                }
            );
        }

        #[test]
        fn test_read_sym() {
            let mut iter = MultiPeek::new("$".chars());
            match read_sym(&mut iter) {
                Ok(s) => assert_eq!(s, "$".to_string()),
                Err(e) => panic!("Failed to read sym: {:?}", e),
            }

            let mut iter = MultiPeek::new("\"AA-BB\"".chars());
            match read_sym(&mut iter) {
                Ok(s) => assert_eq!(s, "\"AA-BB\"".to_string()),
                Err(e) => panic!("Failed to read sym: {:?}", e),
            }

            let mut iter = MultiPeek::new("\"AA%$^${}A-BB\"".chars());
            match read_sym(&mut iter) {
                Ok(s) => assert_eq!(s, "\"AA%$^${}A-BB\"".to_string()),
                Err(e) => panic!("Failed to read sym: {:?}", e),
            }
        }

        #[test]
        fn test_read_next_word() {
            let mut iter = MultiPeek::new("  hellx world   ".chars());
            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("hellx".to_string())
            );
            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("world".to_string())
            );
            assert_eq!(read_until(&mut iter, |c| c.is_whitespace()), None);

            let mut iter = MultiPeek::new(" hellx    world".chars());
            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("hellx".to_string())
            );
            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("world".to_string())
            );
            assert_eq!(read_until(&mut iter, |c| c.is_whitespace()), None);

            let mut iter = MultiPeek::new("$ 123.00".chars());
            iter.peek();
            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("$".to_string())
            );

            let mut empty_iter = MultiPeek::new("   ".chars());
            assert_eq!(read_until(&mut empty_iter, |c| c.is_whitespace()), None);
        }
        #[test]
        fn test_peek_next_word() {
            let mut iter = MultiPeek::new("  hellx world   ".chars());
            assert_eq!(peek_next_word(&mut iter), Some("hellx".to_string()));
            assert_eq!(peek_next_word(&mut iter), Some("world".to_string()));
            assert_eq!(peek_next_word(&mut iter), None);

            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("hellx".to_string())
            );
            assert_eq!(
                read_until(&mut iter, |c| c.is_whitespace()),
                Some("world".to_string())
            );
            assert_eq!(read_until(&mut iter, |c| c.is_whitespace()), None);

            let mut empty_iter = MultiPeek::new("   ".chars());
            assert_eq!(read_until(&mut empty_iter, |c| c.is_whitespace()), None);
        }

        #[test]
        fn test_discard_ws() {
            let mut iter = MultiPeek::new("h".chars());
            discard_ws(&mut iter, true);
            assert_eq!(iter.peek(), Some(&'h'));

            let mut iter = MultiPeek::new(" h".chars());
            discard_ws(&mut iter, true);
            assert_eq!(iter.peek(), Some(&'h'));

            let mut iter = MultiPeek::new("  h".chars());
            discard_ws(&mut iter, true);
            assert_eq!(iter.peek(), Some(&'h'));

            let mut iter = MultiPeek::new("      h".chars());
            discard_ws(&mut iter, true);
            assert_eq!(iter.peek(), Some(&'h'));
            //
            let mut iter = MultiPeek::new("h".chars());
            discard_ws(&mut iter, false);
            assert_eq!(iter.next(), Some('h'));

            let mut iter = MultiPeek::new(" h".chars());
            discard_ws(&mut iter, false);
            assert_eq!(iter.next(), Some('h'));

            let mut iter = MultiPeek::new("  h".chars());
            discard_ws(&mut iter, false);
            assert_eq!(iter.next(), Some('h'));

            let mut iter = MultiPeek::new("      h".chars());
            discard_ws(&mut iter, false);
            assert_eq!(iter.next(), Some('h'));
        }
    }
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

        let at1 = misc::to_datetime(at1);
        let at2 = misc::to_datetime(at2);
        let at3 = misc::to_datetime(at3);

        db.upsert_price(s1, at1, quantity!(100.0, "USD"));
        db.upsert_price(s1, at2, quantity!(105.0, "USD"));
        db.upsert_price(s1, at3, quantity!(110.0, "USD"));
        db.upsert_price(s2, at1, quantity!(1.0, "$"));
        db.upsert_price(s2, at2, quantity!(1.05, "$"));

        assert_eq!(db.latest_price(s1), quantity!(110.0, "USD"));
        assert_eq!(db.price_as_of(s1, at2), Some(quantity!(105.0, "USD")));
        let t = NaiveDate::from_ymd_opt(2022, 12, 31).unwrap();
        assert_eq!(db.price_as_of(s1, misc::to_datetime(t)), None);
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
        let d1 = misc::to_datetime(NaiveDate::from_ymd_opt(2025, 7, 25).unwrap());
        let d2 = misc::to_datetime(NaiveDate::from_ymd_opt(2025, 8, 9).unwrap());
        let d3 = misc::to_datetime(NaiveDate::from_ymd_opt(2025, 8, 28).unwrap());
        let d4 = misc::to_datetime(NaiveDate::from_ymd_opt(2025, 5, 11).unwrap());

        assert_eq!(db.latest_price(s), quantity!(23.69, "$"));
        assert_eq!(db.price_as_of(s, d1), Some(quantity!(20.15, "$")));
        assert_eq!(db.price_as_of(s, d2), Some(quantity!(20.15, "$")));
        assert_eq!(db.price_as_of(s, d3), Some(quantity!(23.69, "$")));
        assert_eq!(db.price_as_of(s, d4), Some(quantity!(20.00, "$")));
    }
}
