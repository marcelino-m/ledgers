use std::str::FromStr;

use chrono::NaiveDate;
use pest::{self, iterators::Pair, Parser};
use pest_derive::Parser;

use crate::commodity::{Commodity, Quantity};
use crate::journal::{self, State, XactDate};
use rust_decimal::{dec, Decimal};

#[derive(Parser)]
#[grammar = "./src/grammar.pest"]
struct LedgerParser;

#[derive(Debug)]
pub enum ParserError {
    InvalidDate,
    Parser(pest::error::Error<Rule>),
}

pub fn parse_journal(content: &String) -> Result<Vec<journal::Xact>, ParserError> {
    let mut journal = match LedgerParser::parse(Rule::journal, &content) {
        Ok(pairs) => pairs,
        Err(err) => return Err(ParserError::Parser(err)),
    };

    let mut xacts = Vec::new();

    let xact_list = journal.next().unwrap().into_inner().next().unwrap();
    for p in xact_list.into_inner() {
        match p.as_rule() {
            Rule::xact => {
                let xact = parse_xact(p)?;
                xacts.push(xact);
            }
            _ => {
                continue;
            }
        }
    }

    Ok(xacts)
}

fn parse_xact(p: Pair<Rule>) -> Result<journal::Xact, ParserError> {
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
                code = Some(parse_text(p));
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

    return Ok(journal::Xact {
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

fn parse_posting(p: Pair<Rule>) -> Result<journal::Posting, ParserError> {
    let mut state = State::None;
    let mut account = String::from("");
    let mut units: Quantity = Default::default();
    let mut ucost: Quantity = Default::default();
    let mut lots: Lots = Default::default();
    let mut comment: Option<String> = None;

    let inner = p.into_inner();

    for p in inner {
        match p.as_rule() {
            Rule::state => {
                state = parse_state(p.as_str());
            }

            Rule::account => account = parse_text(p),
            Rule::units => {
                units = parse_units(p)?;
            }
            Rule::lots => {
                lots = parse_lots(p)?;
            }
            Rule::cost => {
                let mut inner = p.into_inner();
                let is_unitary = match inner.next().unwrap().as_str() {
                    "@" => true,
                    "@@" => false,
                    _ => unreachable!(),
                };

                let tmp = inner.next().unwrap();
                let rcost = parse_units(tmp)?;

                if is_unitary {
                    ucost = rcost;
                    continue;
                }

                if units.s == Commodity::None {
                    panic!("units should be defined at this point");
                }

                ucost = rcost / units.q;
            }
            Rule::comment => comment = Some(parse_comment(p)),
            _ => unreachable!(),
        }
    }

    Ok(journal::Posting {
        state: state,
        account: account,
        units: units,
        ucost: ucost,
        lots_price: lots.price,
        lots_date: lots.date,
        lots_note: lots.note,
        comment: comment,
    })
}

fn parse_units(p: Pair<Rule>) -> Result<Quantity, ParserError> {
    let p = p.into_inner().next().unwrap();
    match p.as_rule() {
        Rule::units_value => Ok(parse_unit_value(p)),
        // TODO: when implemented unit_expression an error could be
        // returned
        _ => unreachable!(),
    }
}

fn parse_unit_value(p: Pair<Rule>) -> Quantity {
    let mut amount = dec!(0.0);
    let mut sym = Commodity::None;

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::ammount => {
                amount = Decimal::from_str(p.as_str()).unwrap();
            }
            Rule::commodity => {
                sym = Commodity::Symbol(String::from(p.as_str()));
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
    pub price: Option<Quantity>,
    pub date: Option<NaiveDate>,
    pub note: Option<String>,
}

fn parse_lots(p: Pair<Rule>) -> Result<Lots, ParserError> {
    let mut note: Option<String> = None;
    let mut price: Option<Quantity> = None;
    let mut date: Option<NaiveDate> = None;

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::lot_note => note = Some(parse_text(p)),
            Rule::lot_date => match parse_date(p) {
                Ok(d) => date = Some(d),
                Err(err) => return Err(err),
            },
            Rule::lot_price => {
                let value_type = p.into_inner().next().unwrap();
                match value_type.as_rule() {
                    Rule::fixing_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
                        price = Some(parse_unit_value(unit_value))
                    }
                    Rule::point_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
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

    Ok(Lots { price, date, note })
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
        "!" => State::Cleared,
        "*" => State::Pending,
        _ => unreachable!(),
    }
}
