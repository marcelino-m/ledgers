use std::str::FromStr;

use chrono::NaiveDate;
use pest::{self, iterators::Pair, Parser};
use pest_derive::Parser;
use rust_decimal::Decimal;

use crate::commodity::{Amount, Quantity};
use crate::journal::{self, LotPrice, State, XactDate};
use crate::prices::{PriceBasis, PriceType};
use crate::symbol::Symbol;

// max number of eliding amount posting per xact
// TODO: define max number of posting per share
const MAX_ELIDING_AMOUNT: usize = 1;

#[derive(Parser)]
#[grammar = "./src/grammar.pest"]
struct LedgerParser;

#[derive(Debug)]
pub enum ParserError {
    InvalidDate,
    Parser(pest::error::Error<Rule>),
    ElidingAmount(usize),
    XactNoBalanced,
}

#[derive(Debug, Default, PartialEq, Eq)]
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
    lot_price: Option<LotPrice>,
    lot_date: Option<NaiveDate>,
    lot_note: Option<String>,

    comment: Option<String>,
}

impl Posting {
    // if qty != None,  self is an eliding amount
    fn to_posting(&self, qty: Option<Quantity>) -> journal::Posting {
        let qty = qty.unwrap_or_else(|| self.quantity.unwrap());

        // If self.uprice and self.lot_price are omitted, then default
        // to 1 in terms of the commodity itself. However, if only one of
        // them is given, they are considered equal by default.
        // We only respect their specific values if both are present.a
        let (uprice, lot_uprice) = match (self.uprice, self.lot_price) {
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
            state: self.state,
            account: self.account.clone(),
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
    fn to_xact(mut self) -> Result<journal::Xact, ParserError> {
        let nel = self.neliding_amount();
        if nel > MAX_ELIDING_AMOUNT {
            return Err(ParserError::ElidingAmount(nel));
        }

        let eliding = self.generate_posting_for_eliding_amount();
        self.postings.retain(|p| !p.quantity.is_none());
        self.postings.extend(eliding);

        let postings: Vec<journal::Posting> =
            self.postings.iter().map(|p| p.to_posting(None)).collect();

        let mut bal = Amount::default();
        for p in postings.iter() {
            bal += p.value();
        }

        if !bal.is_zero() {
            return Err(ParserError::XactNoBalanced);
        }

        Ok(journal::Xact {
            state: self.state,
            code: self.code,
            date: self.date,
            payee: self.payee,
            comment: self.comment,
            postings,
        })
    }

    fn neliding_amount(&self) -> usize {
        self.postings
            .iter()
            .filter(|p| p.quantity.is_none())
            .count()
    }

    fn generate_posting_for_eliding_amount(&self) -> Vec<Posting> {
        let eliding = self.postings.iter().find(|p| p.quantity.is_none());
        let Some(eliding) = eliding else {
            return Vec::new();
        };

        return self
            .postings
            .iter()
            .filter(|p| !p.quantity.is_none())
            .map(|p| {
                let mut cloned = p.clone();
                cloned.account = eliding.account.clone();
                cloned.quantity = cloned.quantity.map(|c| -c);
                cloned
            })
            .collect();
    }
}

pub fn parse_journal(content: &String) -> Result<Vec<journal::Xact>, ParserError> {
    let mut journal = match LedgerParser::parse(Rule::journal, &content) {
        Ok(pairs) => pairs,
        Err(err) => return Err(ParserError::Parser(err)),
    };

    let mut xacts = Vec::new();

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
            _ => {
                continue;
            }
        }
    }

    Ok(xacts)
}

fn parse_xact(p: Pair<Rule>) -> Result<Xact, ParserError> {
    let inner = p.into_inner();

    let mut date: XactDate = Default::default();
    let mut state = State::None;
    let mut code: Option<String> = None;
    let mut comment: Option<String> = None;
    let mut payee: String = Default::default();

    let mut postings = Vec::new();

    for p in inner {
        match p.as_rule() {
            Rule::xact_date => match parse_xact_date(p) {
                Ok(r) => date = r,
                Err(err) => return Err(err),
            },
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

fn parse_xact_date(p: Pair<Rule>) -> Result<XactDate, ParserError> {
    let mut p = p.into_inner();

    let txdate = parse_date(p.next().unwrap())?;
    let efdate = if let Some(op) = p.next() {
        Some(parse_date(op)?)
    } else {
        None
    };

    Ok(XactDate { txdate, efdate })
}

fn parse_date(p: Pair<Rule>) -> Result<NaiveDate, ParserError> {
    let mut inner = p.into_inner();
    let mut inner = inner.next().unwrap().into_inner();

    let y: i32 = inner.next().unwrap().as_str().parse().unwrap();
    let m: u32 = inner.next().unwrap().as_str().parse().unwrap();
    let d: u32 = inner.next().unwrap().as_str().parse().unwrap();

    if let Some(d) = NaiveDate::from_ymd_opt(y, m, d) {
        Ok(d)
    } else {
        Err(ParserError::InvalidDate)
    }
}

fn parse_text(p: Pair<Rule>) -> String {
    String::from(p.as_str())
}

fn parse_posting(p: Pair<Rule>) -> Result<Posting, ParserError> {
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
                let cost = parse_quantity(tmp)?;

                if is_unitary {
                    uprice = Some(cost);
                    continue;
                }

                let Some(ref qty) = qty else {
                    panic!("units should be defined at this point");
                };

                uprice = Some(cost / qty.q);
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
                price: p / qty.clone().unwrap(),
                ptype: price_type,
            },
        }
    });

    Ok(Posting {
        state: state,
        account: account,
        quantity: qty,
        uprice,
        lot_price: ulot,
        lot_date: lots.date,
        lot_note: lots.note,
        comment: comment,
    })
}

fn parse_quantity(p: Pair<Rule>) -> Result<Quantity, ParserError> {
    let p = p.into_inner().next().unwrap();
    match p.as_rule() {
        Rule::units_value => Ok(parse_unit_value(p)),
        // TODO: when implemented unit_expression an error could be
        // returned
        _ => unreachable!(),
    }
}

fn parse_unit_value(p: Pair<Rule>) -> Quantity {
    let mut amount = Decimal::ZERO;
    let mut sym = Symbol::new("");

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::ammount => {
                // TODO: support other format than decimal point
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

fn parse_lots(p: Pair<Rule>) -> Result<Lots, ParserError> {
    let mut note: Option<String> = None;
    let mut price: Option<Quantity> = None;
    let mut price_type: Option<PriceType> = None;
    let mut price_basis: Option<PriceBasis> = None;
    let mut date: Option<NaiveDate> = None;

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::lot_note => note = Some(parse_text(p)),
            Rule::lot_date => match parse_date(p) {
                Ok(d) => date = Some(d),
                Err(err) => return Err(err),
            },
            Rule::lot_price => {
                // TODO: return info about the lot price type, it
                // could be total price or unitary price
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity;
    use pretty_assertions::assert_eq;
    use rust_decimal::dec;

    #[test]
    fn test_parse_xact() -> Result<(), ParserError> {
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
            Err(err) => return Err(ParserError::Parser(err)),
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
                    lot_price: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(50, "LTM")),
                    uprice: Some(quantity!(30.00, "$")),
                    lot_price: None,
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(40, "LTM")),
                    uprice: None,
                    lot_price: Some(LotPrice {
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
                    lot_price: Some(LotPrice {
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
                    lot_price: None,
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
                    state: State::None,
                    account: String::from("Assets:Bank:Checking"),
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
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
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
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
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
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
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
                    state: State::None,
                    account: String::from("Equity:Opening Balances"),
                    quantity: quantity!(-1000.00, "$"),
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
                    state: State::None,
                    account: String::from("Equity:Opening Balances"),
                    quantity: quantity!(-50, "LTM"),
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
                    state: State::None,
                    account: String::from("Equity:Opening Balances"),
                    quantity: quantity!(-40, "LTM"),
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
                    state: State::None,
                    account: String::from("Equity:Opening Balances"),
                    quantity: quantity!(-10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
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
    fn test_parse_xact2() -> Result<(), ParserError> {
        let xact = "\
2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     10 LTM {$30.00} @ $20.00
    * Equity:Opening Balances
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParserError::Parser(err)),
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
                    lot_price: Some(LotPrice {
                        price: quantity!(30.00, "$"),
                        ptype: PriceType::Floating,
                    }),
                    lot_date: None,
                    lot_note: None,
                    comment: None,
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Equity:Opening Balances"),
                    quantity: None,
                    uprice: None,
                    lot_price: None,
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
                    state: State::Pending,
                    account: String::from("Assets:Brokerage"),
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
                    state: State::Cleared,
                    account: String::from("Equity:Opening Balances"),
                    quantity: quantity!(-10, "LTM"),
                    uprice: quantity!(20.00, "$"),
                    lot_uprice: LotPrice {
                        price: quantity!(30.00, "$"),
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
}
