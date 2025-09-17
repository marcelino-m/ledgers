use std::io;
use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use pest::{self, iterators::Pair, Parser};
use pest_derive::Parser;
use rust_decimal::Decimal;

use crate::commodity::{Amount, Quantity};
use crate::journal::{self, AccName, LotPrice, State, XactDate};
use crate::pricedb::{MarketPrice, PriceBasis, PriceType};
use crate::symbol::Symbol;

const MAX_ELIDING_AMOUNT: usize = 1;

#[derive(Parser)]
#[grammar = "./src/grammar.pest"]
struct LedgerParser;

#[derive(Debug)]
pub enum ParseError {
    InvalidDate,
    Parser(pest::error::Error<Rule>),
    ElidingAmount(usize),
    XactNoBalanced,
    IOErr(io::Error),
}

#[derive(Debug, PartialEq, Eq)]
struct Xact {
    state: State,
    code: Option<String>,
    date: XactDate,
    payee: String,
    comment: Option<String>,
    postings: Vec<Posting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Posting {
    // posting state
    state: State,
    // name of the account
    account: String,
    // debits and credits correspond to positive and negative values,
    // respectively.
    quantity: Option<Quantity>,
    // price by unit, here we capture (@ $price) or (@@ $total)
    uprice: Option<Quantity>,
    // lots, lot_price capture {$price} or {=$price}
    lot_uprice: Option<LotPrice>,
    lot_date: Option<NaiveDate>,
    lot_note: Option<String>,

    comment: Option<String>,
}

impl Posting {
    fn to_posting(&self, date: NaiveDate) -> journal::Posting {
        let qty = self.quantity.unwrap();

        // If self.uprice and self.lot_price are omitted, then default
        // to 1 in terms of the commodity itself. However, if only one of
        // them is given, they are considered equal by default.
        // We only respect their specific values if both are present.a
        let (uprice, lot_uprice) = match (self.uprice, self.lot_uprice) {
            (Some(p), Some(lp)) => (p, lp),
            (None, Some(lp)) => (lp.price, lp),
            (Some(p), None) => (
                p,
                LotPrice {
                    price: p,
                    ptype: PriceType::Floating,
                },
            ),
            (None, None) => (
                qty / qty,
                LotPrice {
                    price: qty / qty,
                    ptype: PriceType::Floating,
                },
            ),
        };

        journal::Posting {
            date: date,
            state: self.state,
            acc_name: AccName::from(self.account.clone()),
            quantity: qty,
            uprice: uprice,
            lot_uprice: lot_uprice,
            lot_date: self.lot_date,
            lot_note: self.lot_note.clone(),
            comment: self.comment.clone(),
        }
    }
}

impl Xact {
    fn to_xact(mut self) -> Result<journal::Xact, ParseError> {
        let nel = self.neliding_amount();
        if nel > MAX_ELIDING_AMOUNT {
            return Err(ParseError::ElidingAmount(nel));
        }

        let eliding = self.maybe_remove_eliding();
        let mut postings: Vec<journal::Posting> = self
            .postings
            .iter()
            .map(|p| p.to_posting(self.date.txdate))
            .collect();

        let val: Amount = postings.iter().map(|p| p.book_value()).sum();
        let Some(eliding) = eliding else {
            if !val.is_zero() {
                return Err(ParseError::XactNoBalanced);
            }

            return Ok(journal::Xact {
                state: self.state,
                code: self.code,
                date: self.date,
                payee: self.payee,
                comment: self.comment,
                postings,
            });
        };

        // TODO: fixing value here
        postings.extend(val.iter_quantities().map(|q| {
            let mut p = eliding.clone();
            p.quantity = Some(-q);
            p.to_posting(self.date.txdate)
        }));

        return Ok(journal::Xact {
            state: self.state,
            code: self.code,
            date: self.date,
            payee: self.payee,
            comment: self.comment,
            postings,
        });
    }

    /// Removes and returns the first `Posting` from the `postings`
    /// list where `quantity` is `None`.
    fn maybe_remove_eliding(&mut self) -> Option<Posting> {
        if let Some(pos) = self.postings.iter().position(|p| p.quantity.is_none()) {
            Some(self.postings.remove(pos))
        } else {
            None
        }
    }

    fn neliding_amount(&self) -> usize {
        self.postings
            .iter()
            .filter(|p| p.quantity.is_none())
            .count()
    }
}

pub struct ParsedJounral {
    pub xacts: Vec<journal::Xact>,
    pub market_prices: Vec<MarketPrice>,
}

pub fn parse_journal(content: &String) -> Result<ParsedJounral, ParseError> {
    let mut journal = match LedgerParser::parse(Rule::journal, &content) {
        Ok(pairs) => pairs,
        Err(err) => return Err(ParseError::Parser(err)),
    };

    let mut xacts = Vec::new();
    let mut market_prices = Vec::new();

    let element_list = journal.next().unwrap().into_inner().next().unwrap();
    for p in element_list.into_inner() {
        match p.as_rule() {
            Rule::xact => {
                let xact = parse_xact(p)?;
                let xact = xact.to_xact();

                let Ok(xact) = xact else {
                    return Err(xact.unwrap_err());
                };

                xacts.push(xact);
            }
            Rule::market_price => {
                let mp = parse_market_price(p)?;
                market_prices.push(mp);
            }
            _ => {
                continue;
            }
        }
    }

    Ok(ParsedJounral {
        xacts: xacts,
        market_prices: market_prices,
    })
}

fn parse_xact(p: Pair<Rule>) -> Result<Xact, ParseError> {
    let inner = p.into_inner();

    let mut date = XactDate::default();
    let mut state = State::None;
    let mut code: Option<String> = None;
    let mut comment: Option<String> = None;
    let mut payee: String = Default::default();

    let mut postings = Vec::new();

    for p in inner {
        match p.as_rule() {
            Rule::xact_date => {
                date = parse_xact_date(p)?;
            }
            Rule::state => {
                state = parse_state(p.as_str());
            }
            Rule::code => {
                let rc = parse_text(p.into_inner().next().unwrap());
                code = Some(rc.trim().to_string());
            }
            Rule::payee => payee = String::from(p.as_str()),
            Rule::comment => comment = Some(parse_comment(p)),
            Rule::postings => {
                for p in p.into_inner() {
                    let ps = parse_posting(p)?;
                    postings.push(ps);
                }
            }
            _ => unreachable!(),
        }
    }

    return Ok(Xact {
        state,
        code,
        date,
        payee,
        comment,
        postings,
    });
}

fn parse_xact_date(p: Pair<Rule>) -> Result<XactDate, ParseError> {
    let mut p = p.into_inner();

    let date = p.next().unwrap().into_inner().next().unwrap();
    let txdate = parse_date(date)?;
    let efdate = if let Some(op) = p.next() {
        Some(parse_date(op)?)
    } else {
        None
    };

    Ok(XactDate { txdate, efdate })
}

fn parse_date(p: Pair<Rule>) -> Result<NaiveDate, ParseError> {
    let mut inner = p.into_inner();

    let y: i32 = inner.next().unwrap().as_str().parse().unwrap();
    let m: u32 = inner.next().unwrap().as_str().parse().unwrap();
    let d: u32 = inner.next().unwrap().as_str().parse().unwrap();

    if let Some(d) = NaiveDate::from_ymd_opt(y, m, d) {
        Ok(d)
    } else {
        Err(ParseError::InvalidDate)
    }
}

fn parse_text(p: Pair<Rule>) -> String {
    String::from(p.as_str())
}

fn parse_posting(p: Pair<Rule>) -> Result<Posting, ParseError> {
    let mut state = State::None;
    let mut account = String::from("");
    let mut qty: Option<Quantity> = None;
    let mut uprice: Option<Quantity> = None;
    let mut lots = Lots::default();
    let mut comment: Option<String> = None;

    let inner = p.into_inner();

    for p in inner {
        match p.as_rule() {
            Rule::state => {
                state = parse_state(p.as_str());
            }

            Rule::account => account = parse_text(p),
            Rule::quantity => {
                qty = Some(parse_quantity(p)?);
            }
            Rule::lots => {
                lots = parse_lots(p)?;
            }
            Rule::price => {
                let mut inner = p.into_inner();
                let is_unitary = match inner.next().unwrap().as_str() {
                    "@" => true,
                    "@@" => false,
                    _ => unreachable!(),
                };

                let tmp = inner.next().unwrap();
                let price = parse_quantity(tmp)?;

                if is_unitary {
                    uprice = Some(price);
                    continue;
                }

                let Some(ref qty) = qty else {
                    panic!("units should be defined at this point");
                };

                uprice = Some(price / qty.q.abs());
            }
            Rule::comment => comment = Some(parse_comment(p)),
            _ => unreachable!(),
        }
    }

    let ulot = lots.price.map(|p| {
        let price_base = lots.price_basis.unwrap();
        let price_type = lots.price_type.unwrap();
        match price_base {
            PriceBasis::PerUnit => LotPrice {
                price: p,
                ptype: price_type,
            },
            PriceBasis::Total => LotPrice {
                price: p / qty.unwrap().q.abs(),
                ptype: price_type,
            },
        }
    });

    Ok(Posting {
        state: state,
        account: account,
        quantity: qty,
        uprice: uprice,
        lot_uprice: ulot,
        lot_date: lots.date,
        lot_note: lots.note,
        comment: comment,
    })
}

fn parse_quantity(p: Pair<Rule>) -> Result<Quantity, ParseError> {
    let p = p.into_inner().next().unwrap();
    match p.as_rule() {
        Rule::units_value => Ok(parse_unit_value(p)),
        // TODO: when implemented unit_expression an error could be
        // returned
        _ => unreachable!(),
    }
}

// TODO: this function should return a Result<Quantity, ParserError>
// amount could be malformed for example 1,1,1 y valid amount
fn parse_unit_value(p: Pair<Rule>) -> Quantity {
    let mut amount = Decimal::ZERO;
    let mut sym = Symbol::new("");

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::ammount => {
                // TODO: [DECIMAL] support other format than decimal point
                // notation
                let str = p.as_str().replace(",", "");
                amount = Decimal::from_str(&str).unwrap();
            }
            Rule::commodity => {
                sym = Symbol::new(p.as_str());
            }

            _ => {
                unreachable!()
            }
        }
    }

    Quantity { q: amount, s: sym }
}

#[derive(Debug, Default)]
pub struct Lots {
    price: Option<Quantity>,
    price_type: Option<PriceType>,
    price_basis: Option<PriceBasis>,

    date: Option<NaiveDate>,
    note: Option<String>,
}

fn parse_lots(p: Pair<Rule>) -> Result<Lots, ParseError> {
    let mut note: Option<String> = None;
    let mut price: Option<Quantity> = None;
    let mut price_type: Option<PriceType> = None;
    let mut price_basis: Option<PriceBasis> = None;
    let mut date: Option<NaiveDate> = None;

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::lot_note => note = Some(parse_text(p)),
            Rule::lot_date => {
                let t = p.into_inner().next().unwrap();
                match parse_date(t) {
                    Ok(d) => date = Some(d),
                    Err(err) => return Err(err),
                }
            }
            Rule::lot_price => {
                let value_type = p.into_inner().next().unwrap();
                match value_type.as_rule() {
                    Rule::fixing_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
                        price_type = Some(PriceType::Static);
                        price_basis = Some(PriceBasis::PerUnit);
                        price = Some(parse_unit_value(unit_value))
                    }
                    Rule::per_unit_point_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
                        price_type = Some(PriceType::Floating);
                        price_basis = Some(PriceBasis::PerUnit);
                        price = Some(parse_unit_value(unit_value))
                    }
                    Rule::total_point_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
                        price_type = Some(PriceType::Floating);
                        price_basis = Some(PriceBasis::Total);
                        price = Some(parse_unit_value(unit_value))
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                unreachable!();
            }
        }
    }

    Ok(Lots {
        price,
        price_type,
        price_basis,
        date,
        note,
    })
}

fn parse_comment(p: Pair<Rule>) -> String {
    let mut lines = Vec::new();
    for p in p.into_inner() {
        lines.push(parse_text(p));
    }

    lines.join("")
}

fn parse_state(s: &str) -> State {
    match s {
        "!" => State::Pending,
        "*" => State::Cleared,
        _ => unreachable!(),
    }
}

fn parse_market_price(p: Pair<Rule>) -> Result<MarketPrice, ParseError> {
    let inner = p.into_inner();

    let mut date = None;
    let mut time = None;
    let mut sym = Symbol::new("");
    let mut price = None;

    for p in inner {
        match p.as_rule() {
            Rule::date => {
                date = Some(parse_date(p)?);
            }
            Rule::time => match NaiveTime::from_str(p.as_str()) {
                Ok(t) => time = Some(t),
                Err(_) => return Err(ParseError::InvalidDate),
            },
            Rule::commodity => {
                sym = Symbol::new(p.as_str());
            }
            Rule::units_value => {
                price = Some(parse_unit_value(p));
            }
            _ => unreachable!(),
        }
    }

    let date = date.unwrap();
    let dt = if let Some(t) = time {
        NaiveDateTime::new(date, t)
    } else {
        NaiveDateTime::new(date, NaiveTime::from_hms_opt(0, 0, 0).unwrap())
    };

    Ok(MarketPrice {
        date_time: dt,
        sym: sym,
        price: price.unwrap(),
    })
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use rust_decimal::dec;

    use super::*;
    use crate::quantity;

    #[test]
    fn test_parse_xact() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * Checking balance
    Assets:Bank:Checking              $1000.00
    Assets:Brokerage                     50 LTM @ $30.00
    Assets:Brokerage                     40 LTM {$30.00}
    Assets:Brokerage                     10 LTM {$30.00} @ $20.00
    Equity:Opening Balances
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;

        let expected = Xact {
            state: State::Cleared,
            code: None,
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                Posting {
                    state: State::None,
                    account: String::from("Assets:Bank:Checking"),
                    quantity: Some(quantity!(1000.00, "$")),
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(50, "LTM")),
                    uprice: Some(quantity!(30.00, "$")),
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(40, "LTM")),
                    uprice: None,
                    lot_uprice: Some(LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    }),
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(10, "LTM")),
                    uprice: Some(quantity!(20.00, "$")),
                    lot_uprice: Some(LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    }),
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::None,
                    account: String::from("Equity:Opening Balances"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: None,
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Assets:Bank:Checking"),
                    quantity: quantity!(1000.00, "$"),
                    uprice: quantity!(1, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(50, "LTM"),
                    uprice: quantity!(30.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(40, "LTM"),
                    uprice: quantity!(30.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                // generate eliding amount
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Equity:Opening Balances"),
                    quantity: quantity!(-4000.00, "$"),
                    uprice: quantity!(1, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed.to_xact()?, expected);

        Ok(())
    }

    #[test]
    fn test_parse_xact2() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     10 LTM [2025/08/29] {$30.00} @ $20.00
    * Assets:Checking
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;

        let expected = Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                Posting {
                    state: State::Pending,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(10, "LTM")),
                    uprice: Some(quantity!(20.00, "$")),
                    lot_uprice: Some(LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    }),
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Assets:Checking"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Pending,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                // generate eliding amount
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Cleared,
                    acc_name: AccName::from("Assets:Checking"),
                    quantity: quantity!(-300, "$"),
                    uprice: quantity!(1, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed.to_xact()?, expected);

        Ok(())
    }

    /// Same like test2 but using `total` lot price
    #[test]
    fn test_parse_xact3() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     10 LTM {{$300.00}} [2025/08/29]  @ $20.00
    * Assets:Cash
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;

        let expected = Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                Posting {
                    state: State::Pending,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(10, "LTM")),
                    uprice: Some(quantity!(20.00, "$")),
                    lot_uprice: Some(LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    }),
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Assets:Cash"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Pending,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                // generate eliding amount
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Cleared,
                    acc_name: AccName::from("Assets:Cash"),
                    quantity: quantity!(-300, "$"),
                    uprice: quantity!(1, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed.to_xact()?, expected);

        Ok(())
    }

    #[test]
    fn test_parse_xact4() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     -10 LTM {{$300.00}} [2025/08/29] @@ $200.00
    * Assets:Cash
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;

        let expected = Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                Posting {
                    state: State::Pending,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(-10, "LTM")),
                    uprice: Some(quantity!(20.00, "$")),
                    lot_uprice: Some(LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    }),
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Assets:Cash"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Pending,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(-10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                // generate eliding amount
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Cleared,
                    acc_name: AccName::from("Assets:Cash"),
                    quantity: quantity!(300, "$"),
                    uprice: quantity!(1, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed.to_xact()?, expected);

        Ok(())
    }

    #[test]
    fn test_parser_journal() -> Result<(), ParseError> {
        let jf = "\
P 2025/07/25 LTM  $ 20.15
P 2025/08/09  12:00:00 LTM $ 21.10

2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     -10 LTM {{$300.00}} [2025/08/29] @@ $200.00
    * Assets:Cash

P 2025/08/28 LTM  $ 23.69
";
        let parsed = parse_journal(&jf.to_string())?;
        let expected = journal::Xact {
            state: State::Cleared,
            code: Some(String::from("#1985")),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: None,
            postings: vec![
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Pending,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(-10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: Some(NaiveDate::from_ymd_opt(2025, 8, 29).unwrap()),
                    lot_note: None,
                    comment: None,
                },
                // generate eliding amount
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::Cleared,
                    acc_name: AccName::from("Assets:Cash"),
                    quantity: quantity!(300, "$"),
                    uprice: quantity!(1, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "$"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
            ],
        };

        assert_eq!(parsed.xacts, vec![expected]);

        let expected = vec![
            journal::MarketPrice {
                date_time: NaiveDate::from_ymd_opt(2025, 7, 25)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                sym: Symbol::new("LTM"),
                price: quantity!(20.15, "$"),
            },
            journal::MarketPrice {
                date_time: NaiveDate::from_ymd_opt(2025, 8, 9)
                    .unwrap()
                    .and_hms_opt(12, 0, 0)
                    .unwrap(),
                sym: Symbol::new("LTM"),
                price: quantity!(21.10, "$"),
            },
            journal::MarketPrice {
                date_time: NaiveDate::from_ymd_opt(2025, 8, 28)
                    .unwrap()
                    .and_hms_opt(0, 0, 0)
                    .unwrap(),
                sym: Symbol::new("LTM"),
                price: quantity!(23.69, "$"),
            },
        ];

        assert_eq!(parsed.market_prices, expected);

        Ok(())
    }
}
