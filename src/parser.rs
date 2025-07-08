use chrono::NaiveDate;
use pest::{self, iterators::Pair, Parser};
use pest_derive::Parser;

use crate::commodity::{Commodity, Unit};
use crate::journal::State;

#[derive(Parser)]
#[grammar = "./src/grammar.pest"]
struct LedgerParser;

#[derive(Debug)]
pub enum ParserError {
    InvalidDate,
    Parser(pest::error::Error<Rule>),
}

pub fn parse_journal(content: &String) -> Result<Vec<Xact>, ParserError> {
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

#[derive(Debug)]
pub struct Xact {
    pub state: State,
    pub code: Option<String>,
    pub date: Option<XactDate>,
    pub payee: Option<String>,
    pub comment: Option<String>,
    pub postings: Vec<Posting>,
}

fn parse_xact(p: Pair<Rule>) -> Result<Xact, ParserError> {
    let inner = p.into_inner();

    let mut date: Option<XactDate> = None;
    let mut state = State::None;
    let mut code: Option<String> = None;
    let mut comment: Option<String> = None;
    let mut payee: Option<String> = None;

    let mut postings = Vec::new();

    for p in inner {
        match p.as_rule() {
            Rule::xact_date => match parse_xact_date(p) {
                Ok(r) => date = Some(r),
                Err(err) => return Err(err),
            },
            Rule::state => {
                state = parse_state(p.as_str());
            }
            Rule::code => {
                code = Some(parse_text(p));
            }
            Rule::payee => payee = Some(String::from(p.as_str())),
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

#[derive(Debug)]
pub struct XactDate {
    pub txdate: NaiveDate,
    pub efdate: Option<NaiveDate>,
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

#[derive(Debug)]
pub struct Posting {
    pub state: State,
    pub account: String,
    //Debits and credits correspond to positive and negative values,
    // respectively.
    pub units: Option<Unit>,
    // cost by unit
    pub ucost: Option<Unit>,
    pub lots: Option<Lots>,
    pub comment: Option<String>,
}

fn parse_posting(p: Pair<Rule>) -> Result<Posting, ParserError> {
    let mut state = State::None;
    let mut account = String::new();
    let mut units: Option<Unit> = None;
    let mut ucost: Option<Unit> = None;
    let mut lots: Option<Lots> = None;
    let mut comment: Option<String> = None;

    let inner = p.into_inner();

    for p in inner {
        match p.as_rule() {
            Rule::state => {
                state = parse_state(p.as_str());
            }

            Rule::account => account = parse_text(p),
            Rule::units => {
                units = Some(parse_units(p)?);
            }
            Rule::lots => {
                lots = Some(parse_lots(p)?);
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
                    ucost = Some(rcost);
                    continue;
                }

                let Some(ref u) = units else {
                    panic!("units should be defined at this point");
                };

                ucost = Some(rcost / u.q);
            }
            Rule::comment => comment = Some(parse_comment(p)),
            _ => unreachable!(),
        }
    }

    Ok(Posting {
        state,
        account,
        units,
        ucost,
        lots,
        comment,
    })
}

fn parse_units(p: Pair<Rule>) -> Result<Unit, ParserError> {
    let p = p.into_inner().next().unwrap();
    match p.as_rule() {
        Rule::units_value => Ok(parse_unit_value(p)),
        // TODO: when implemented unit_expression an error could be
        // returned
        _ => unreachable!(),
    }
}

fn parse_unit_value(p: Pair<Rule>) -> Unit {
    let mut amount: f64 = 0.0;
    let mut sym = Commodity::None;

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::ammount => {
                amount = p.as_str().parse().unwrap();
            }
            Rule::commodity => {
                sym = Commodity::Symbol(String::from(p.as_str()));
            }

            _ => {
                unreachable!()
            }
        }
    }

    Unit { q: amount, s: sym }
}

#[derive(Debug)]
pub struct Lots {
    pub price: Option<Unit>,
    pub date: Option<NaiveDate>,
    pub note: Option<String>,
}

fn parse_lots(p: Pair<Rule>) -> Result<Lots, ParserError> {
    let mut note: Option<String> = None;
    let mut price: Option<Unit> = None;
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
