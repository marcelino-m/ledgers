use std::str::FromStr;

use chrono::NaiveDate;
use pest::{self, iterators::Pair, Parser};
use pest_derive::Parser;
use rust_decimal::Decimal;

use crate::commodity::{Amount, Quantity};
use crate::journal::{self, State, XactDate};
use crate::prices::{PriceBasis, PriceType};
use crate::symbol::Symbol;

// max number of eliding amount posting per xact
// TODO: define max number of posting per share
const MAX_ELIDING_AMOUNT: u16 = 1;

#[derive(Parser)]
#[grammar = "./src/grammar.pest"]
struct LedgerParser;

#[derive(Debug)]
pub enum ParserError {
    InvalidDate,
    Parser(pest::error::Error<Rule>),
    ElidingAmount(u16),
    XactNoBalanced,
}

#[derive(Debug, Default)]
struct Xact {
    state: State,
    code: Option<String>,
    date: XactDate,
    payee: String,
    comment: Option<String>,
    postings: Vec<Posting>,
}

#[derive(Debug)]
struct Posting {
    // posting state
    state: State,
    // name of the account
    account: String,
    // debits and credits correspond to positive and negative values,
    // respectively. Each xact in the journal only can have one
    // commodity quantity
    quantity: Option<Quantity>,
    // cost by unit
    ucost: Option<Quantity>, // TODO: rename to price
    // lots
    lots_price: Option<LotPrice>,
    lots_date: Option<NaiveDate>,
    lots_note: Option<String>,

    comment: Option<String>,
}

#[derive(Debug)]
struct LotPrice {
    pub price: Quantity,
    pub ptype: PriceType,
    pub pbasis: PriceBasis,
}

impl Posting {
    fn to_posting(self, qty: Option<Amount>) -> journal::Posting {
        if let Some(qty) = qty {
            // this indicate that this posting have eliding amount
            return journal::Posting {
                state: self.state,
                account: self.account,
                quantity: qty,
                ucost: self.ucost.map(|u| u.to_amount()),
                lots_price: None,
                lots_date: None,
                lots_note: None,
                comment: self.comment,
            };
        }

        let lots_price = self.lots_price.map(|l| {
            let mut price = l.price;
            if l.pbasis == PriceBasis::Total {
                let qty = self.quantity.unwrap();
                price = price / qty;
            }

            journal::LotPrice {
                price: price,
                pbasis: l.ptype,
            }
        });

        journal::Posting {
            state: self.state,
            account: self.account,
            quantity: self.quantity.unwrap().to_amount(),
            ucost: self.ucost.map(|u| u.to_amount()),
            lots_price: lots_price,
            lots_date: self.lots_date,
            lots_note: self.lots_note,
            comment: self.comment,
        }
    }
}

impl Xact {
    fn to_xact(self) -> Result<journal::Xact, ParserError> {
        let neliding = self.neliding_amount();
        if neliding > MAX_ELIDING_AMOUNT {
            return Err(ParserError::ElidingAmount(neliding));
        }

        let posting = if neliding == 0 {
            self.postings
                .into_iter()
                .map(|p| p.to_posting(None))
                .collect()
        } else {
            let (id, qty) = self.calc_eliding_amount();
            let mut posting = Vec::with_capacity(self.postings.len());
            for (i, p) in self.postings.into_iter().enumerate() {
                if i != id {
                    posting.push(p.to_posting(None));
                } else {
                    posting.push(p.to_posting(Some(qty.clone())));
                }
            }
            posting
        };

        let mut bal = Amount::default();
        for p in posting.iter() {
            bal += &p.quantity
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
            postings: posting,
        })
    }

    fn neliding_amount(&self) -> u16 {
        let mut count = 0;
        for p in self.postings.iter() {
            if let None = p.quantity {
                count += 1;
            };
        }
        count
    }

    fn calc_eliding_amount(&self) -> (usize, Amount) {
        let mut idx = 0;
        let mut sum = Amount::default();
        for (i, p) in self.postings.iter().enumerate() {
            let Some(q) = p.quantity else {
                idx = i;
                continue;
            };

            sum -= q;
        }

        (idx, sum)
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
    let mut ucost: Option<Quantity> = None;
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
            Rule::cost => {
                let mut inner = p.into_inner();
                let is_unitary = match inner.next().unwrap().as_str() {
                    "@" => true,
                    "@@" => false,
                    _ => unreachable!(),
                };

                let tmp = inner.next().unwrap();
                let cost = parse_quantity(tmp)?;

                if is_unitary {
                    ucost = Some(cost);
                    continue;
                }

                let Some(ref qty) = qty else {
                    panic!("units should be defined at this point");
                };

                ucost = Some(cost / qty.q);
            }
            Rule::comment => comment = Some(parse_comment(p)),
            _ => unreachable!(),
        }
    }

    // if have lots.price must have price_basis and price_type
    let lotprice = if let Some(price) = lots.price {
        Some(LotPrice {
            price,
            pbasis: lots.price_basis.unwrap(),
            ptype: lots.price_type.unwrap(),
        })
    } else {
        None
    };

    Ok(Posting {
        state: state,
        account: account,
        quantity: qty,
        ucost: ucost,
        lots_price: lotprice,
        lots_date: lots.date,
        lots_note: lots.note,
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
