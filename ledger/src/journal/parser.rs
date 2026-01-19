use std::collections::HashMap;
use std::io;
use std::str::FromStr;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use pest::{self, Parser, iterators::Pair};
use pest_derive::Parser;
use rust_decimal::Decimal;

use crate::amount::Amount;
use crate::journal::{self, AccName, LotPrice, State, XactDate};
use crate::quantity::Quantity;

use crate::parser_number::{self, NumberFormat};
use crate::pricedb::{MarketPrice, PriceBasis, PriceType};
use crate::symbol::Symbol;
use crate::tags::Tag;

const MAX_ELIDING_AMOUNT: usize = 1;

#[derive(Parser)]
#[grammar = "./src/grammar.pest"]
struct LedgerParser;

#[derive(Debug)]
pub enum ParseError {
    InvalidDate,
    InvalidNumber(String),
    Parser(pest::error::Error<Rule>),
    ElidingAmount(usize),
    XactNoBalanced,
    IOErr(io::Error),
}

#[derive(Debug, PartialEq, Eq)]
struct Xact {
    state: State,
    code: String,
    date: XactDate,
    payee: String,
    comment: String,
    tags: Vec<Tag>,
    vtags: HashMap<Tag, String>,
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
    lot_note: String,

    comment: String,
    tags: Vec<Tag>,
    vtags: HashMap<Tag, String>,
}

impl Posting {
    fn into_posting(self, date: NaiveDate) -> journal::Posting {
        let quantity = self.quantity.unwrap();

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
                quantity.to_unit(),
                LotPrice {
                    price: quantity.to_unit(),
                    ptype: PriceType::Floating,
                },
            ),
        };

        journal::Posting {
            date,
            state: self.state,
            acc_name: AccName::from(self.account),
            quantity,
            uprice,
            lot_uprice,
            lot_date: self.lot_date,
            lot_note: self.lot_note,
            comment: self.comment,
            tags: self.tags,
            vtags: self.vtags,
        }
    }
}

impl Xact {
    fn into_xact(mut self) -> Result<journal::Xact, ParseError> {
        let nel = self.neliding_amount();
        if nel > MAX_ELIDING_AMOUNT {
            return Err(ParseError::ElidingAmount(nel));
        }

        let eliding = self.maybe_remove_eliding();
        let mut postings: Vec<journal::Posting> = self
            .postings
            .into_iter()
            .map(|p| p.into_posting(self.date.txdate))
            .collect();

        let bal: Amount = postings.iter().map(|p| p.book_value()).sum();
        match eliding {
            Some(eliding) => {
                postings.extend(bal.iter_quantities().map(|q| {
                    let mut p = eliding.clone();
                    p.quantity = Some(-q);
                    p.into_posting(self.date.txdate)
                }));
            }
            None => {
                match bal.arity() {
                    n if n != 0 && n != 2 => {
                        return Err(ParseError::XactNoBalanced);
                    }
                    2 => {
                        // balance must be in the form nX - mY
                        let p: Decimal = bal.iter_quantities().map(|qty| qty.q).product();
                        if p > Decimal::ZERO {
                            return Err(ParseError::XactNoBalanced);
                        }
                    }
                    _ => {}
                }
            }
        }

        if bal.arity() == 2 {
            Xact::fill_inferred_prices(&mut postings, bal)
        }

        let xact = journal::Xact {
            state: self.state,
            code: self.code,
            date: self.date,
            payee: self.payee,
            comment: self.comment,
            postings,
            tags: self.tags,
            vtags: self.vtags,
        };

        Ok(xact)
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

    /// count how many postings with an explicit amount/quantity
    fn neliding_amount(&self) -> usize {
        self.postings
            .iter()
            .filter(|p| p.quantity.is_none())
            .count()
    }

    /// In a xact with a balance in the form nA - mB, try to guess
    /// which one is the primary commodity of the xact
    fn guess_primary(ps: &Vec<journal::Posting>, a: Quantity, b: Quantity) -> (Quantity, Quantity) {
        let fs = ps[0].quantity.s;
        if a.s == fs {
            return (b, a);
        } else if b.s == fs {
            return (a, b);
        }

        for p in ps {
            if p.uprice.s != p.quantity.s {
                if p.uprice.s == a.s {
                    return (a, b);
                } else if p.uprice.s == b.s {
                    return (b, a);
                }
            }
        }
        unreachable!()
    }

    /// find postings with secondary commodity having its uprice's
    /// equal to itself and replace it's in terms of the primary
    /// commodity
    fn fill_inferred_prices(postings: &mut Vec<journal::Posting>, bal: Amount) {
        let mut iter = bal.iter_quantities();
        let a = iter.next().unwrap();
        let b = iter.next().unwrap();
        let (pri, sec) = Xact::guess_primary(&postings, a, b);

        postings.iter_mut().for_each(|p| {
            let up = p.uprice;
            let q = p.quantity;
            if q.s != up.s {
                return;
            }

            if q.s == sec.s {
                let psec = (pri / sec).abs();
                p.uprice = psec;
                p.lot_uprice.price = psec;
            }

            if p.lot_uprice.price.s == sec.s {}
        });
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
                let xact = xact.into_xact();

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
        xacts,
        market_prices,
    })
}

fn parse_xact(p: Pair<Rule>) -> Result<Xact, ParseError> {
    let inner = p.into_inner();

    let mut date = XactDate::default();
    let mut state = State::None;
    let mut code = String::new();
    let mut comment = String::new();
    let mut payee: String = Default::default();
    let mut tags = Vec::new();
    let mut vtags = HashMap::new();
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
                code = rc.trim().to_string()
            }
            Rule::payee => payee = String::from(p.as_str()),
            Rule::comment => {
                (comment, tags, vtags) = parse_comment(p);
            }
            Rule::postings => {
                for p in p.into_inner() {
                    let ps = parse_posting(p)?;
                    postings.push(ps);
                }
            }
            _ => unreachable!(),
        }
    }

    Ok(Xact {
        state,
        code,
        date,
        payee,
        comment,
        tags,
        vtags,
        postings,
    })
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

/// Parses tags from a given text.
/// Tags are expected to be in the format `:tag:` or `:tag1:tag2:`.
fn parse_tags(line: &str) -> Result<Vec<Tag>, ParseError> {
    if line.is_empty() || !line.contains(":") {
        return Ok(Vec::new());
    }

    let mut tags = Vec::new();
    let mut curr = String::new();
    let mut found = false;

    for c in line.chars() {
        if found {
            if c.is_whitespace() {
                found = false;
                curr.clear();
                continue;
            }

            if c != ':' {
                curr.push(c);
                continue;
            }

            if c == ':' {
                if curr.is_empty() {
                    continue;
                }
                let tag = Tag::new(&curr);
                tags.push(tag);
                curr.clear();
                continue;
            }
        }

        if c == ':' {
            found = true;
            continue;
        }
    }

    Ok(tags)
}

/// Parases a value tag from a given line. Value tags are expected to
/// be in the format `tag: some value`.
fn parse_vtags(line: &str) -> Result<Option<(Tag, String)>, ParseError> {
    let line = line.trim();

    let mut curr = String::new();
    let mut found = true;
    let mut iter = line.chars().peekable();
    while let Some(c) = iter.next() {
        if found {
            if c.is_whitespace() {
                found = false;
                curr.clear();
                continue;
            }

            if c != ':' {
                curr.push(c);
                continue;
            }
            if c == ':' {
                let Some(c) = iter.peek() else {
                    break;
                };

                if *c != ' ' {
                    curr.clear();
                    found = false;
                    continue;
                }

                iter.next();
                return Ok(Some((Tag::new(&curr), iter.collect())));
            }
        }

        if c == ' ' {
            while let Some(nc) = iter.peek() {
                if *nc == ' ' {
                    iter.next();
                    continue;
                }
                break;
            }
            found = true;
            continue;
        }
    }

    Ok(None)
}

fn parse_posting(p: Pair<Rule>) -> Result<Posting, ParseError> {
    let mut state = State::None;
    let mut account = String::from("");
    let mut quantity: Option<Quantity> = None;
    let mut uprice: Option<Quantity> = None;
    let mut lots = Lots::default();
    let mut comment = String::new();
    let mut tags = Vec::new();
    let mut vtags = HashMap::new();
    let inner = p.into_inner();

    for p in inner {
        match p.as_rule() {
            Rule::state => {
                state = parse_state(p.as_str());
            }

            Rule::account => account = parse_text(p),
            Rule::quantity => {
                quantity = Some(parse_quantity(p)?);
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

                let Some(ref qty) = quantity else {
                    panic!("units should be defined at this point");
                };

                uprice = Some(price / qty.q.abs());
            }
            Rule::comment => {
                (comment, tags, vtags) = parse_comment(p);
            }
            _ => unreachable!(),
        }
    }

    let lot_uprice = lots.price.map(|p| {
        let price_base = lots.price_basis.unwrap();
        let price_type = lots.price_type.unwrap();
        match price_base {
            PriceBasis::PerUnit => LotPrice {
                price: p,
                ptype: price_type,
            },
            PriceBasis::Total => LotPrice {
                price: p / quantity.unwrap().q.abs(),
                ptype: price_type,
            },
        }
    });

    Ok(Posting {
        state,
        account,
        quantity,
        uprice,
        lot_uprice,
        lot_date: lots.date,
        lot_note: lots.note,
        comment,
        vtags,
        tags,
    })
}

fn parse_quantity(p: Pair<Rule>) -> Result<Quantity, ParseError> {
    let p = p.into_inner().next().unwrap();
    match p.as_rule() {
        Rule::units_value => parse_unit_value(p),
        // TODO: when implemented unit_expression an error could be
        // returned
        _ => unreachable!(),
    }
}

// TODO: this function should return a Result<Quantity, ParserError>
// amount could be malformed for example 1,1,1 y valid amount
fn parse_unit_value(p: Pair<Rule>) -> Result<Quantity, ParseError> {
    let mut amount = Decimal::ZERO;
    let mut sym = Symbol::new("");

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::ammount => match parser_number::parse(p.as_str().trim(), NumberFormat::Us) {
                Some(n) => amount = n,
                None => {
                    return Err(ParseError::InvalidNumber(p.as_str().to_string()));
                }
            },
            Rule::commodity => {
                sym = Symbol::new(p.as_str());
            }
            _ => {
                unreachable!()
            }
        }
    }

    Ok(Quantity { q: amount, s: sym })
}

#[derive(Debug, Default)]
pub struct Lots {
    price: Option<Quantity>,
    price_type: Option<PriceType>,
    price_basis: Option<PriceBasis>,

    date: Option<NaiveDate>,
    note: String,
}

fn parse_lots(p: Pair<Rule>) -> Result<Lots, ParseError> {
    let mut note = String::new();
    let mut price: Option<Quantity> = None;
    let mut price_type: Option<PriceType> = None;
    let mut price_basis: Option<PriceBasis> = None;
    let mut date: Option<NaiveDate> = None;

    for p in p.into_inner() {
        match p.as_rule() {
            Rule::lot_note => note = parse_text(p),
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
                        price = Some(parse_unit_value(unit_value)?);
                    }
                    Rule::per_unit_point_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
                        price_type = Some(PriceType::Floating);
                        price_basis = Some(PriceBasis::PerUnit);
                        price = Some(parse_unit_value(unit_value)?);
                    }
                    Rule::total_point_value => {
                        let unit_value = value_type.into_inner().next().unwrap();
                        price_type = Some(PriceType::Floating);
                        price_basis = Some(PriceBasis::Total);
                        price = Some(parse_unit_value(unit_value)?)
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

fn parse_comment(p: Pair<Rule>) -> (String, Vec<Tag>, HashMap<Tag, String>) {
    let mut lines = Vec::new();
    let mut tags = Vec::new();
    let mut vtags = HashMap::new();
    for p in p.into_inner() {
        let txt = parse_text(p);
        match parse_tags(&txt) {
            Ok(ts) => tags.extend(ts),
            Err(_) => {
                // TODO: handle this error
            }
        }
        match parse_vtags(&txt) {
            Ok(vt) => {
                if let Some((tag, val)) = vt {
                    vtags.insert(tag, val);
                }
            }
            Err(_) => {
                // TODO: handle this error
            }
        }

        lines.push(txt);
    }

    (lines.join("\n"), tags, vtags)
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
                price = Some(parse_unit_value(p)?);
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
2004/05/11 * Checking balance  ; :XTag:
    Assets:Bank:Checking              $1,000.00  ; :Tag1: Tag2: Value one
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
            code: String::new(),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::from(":XTag:"),
            tags: vec![Tag::new("XTag")],
            vtags: HashMap::new(),
            postings: vec![
                Posting {
                    state: State::None,
                    account: String::from("Assets:Bank:Checking"),
                    quantity: Some(quantity!(1000.00, "$")),
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::from(":Tag1: Tag2: Value one"),
                    tags: vec![Tag::new("Tag1")],
                    vtags: [(Tag::new("Tag2"), String::from("Value one"))]
                        .into_iter()
                        .collect(),
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(50, "LTM")),
                    uprice: Some(quantity!(30.00, "$")),
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
                Posting {
                    state: State::None,
                    account: String::from("Equity:Opening Balances"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: String::new(),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::from(":XTag:"),
            tags: vec![Tag::new("XTag")],
            vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::from(":Tag1: Tag2: Value one"),
                    tags: vec![Tag::new("Tag1")],
                    vtags: [(Tag::new("Tag2"), String::from("Value one"))]
                        .into_iter()
                        .collect(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed.into_xact()?, expected);

        Ok(())
    }

    #[test]
    fn test_parse_xact2() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * ( #1985 ) Checking balance
  ; TagVal: Suma was great, but ma was blind
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
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::from("TagVal: Suma was great, but ma was blind"),
            tags: Vec::new(),
            vtags: [(
                Tag::new("TagVal"),
                String::from("Suma was great, but ma was blind"),
            )]
            .into_iter()
            .collect(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Assets:Checking"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::from("TagVal: Suma was great, but ma was blind"),
            tags: Vec::new(),
            vtags: [(
                Tag::new("TagVal"),
                String::from("Suma was great, but ma was blind"),
            )]
            .into_iter()
            .collect(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed.into_xact()?, expected);

        Ok(())
    }

    /// Same like test2 but using `total` lot price
    #[test]
    fn test_parse_xact3() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * ( #1985 ) Checking balance
    ! Assets:Brokerage                     10 LTM {{$300.00}} [2025/08/29]  @ $20.00
    * Assets:Cash  ; :SuTag:MaTag:
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;

        let expected = Xact {
            state: State::Cleared,
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Assets:Cash"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::from(":SuTag:MaTag:"),
                    tags: vec![Tag::new("SuTag"), Tag::new("MaTag")],
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::from(":SuTag:MaTag:"),
                    tags: vec![Tag::new("SuTag"), Tag::new("MaTag")],
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed.into_xact()?, expected);

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
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
                Posting {
                    state: State::Cleared,
                    account: String::from("Assets:Cash"),
                    quantity: None,
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed.into_xact()?, expected);

        Ok(())
    }

    #[test]
    fn test_parse_xact5() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * Checking balance
    Assets:Brokerage      1 X
    Assets:Checking      -1 Y
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;

        let expected = Xact {
            state: State::Cleared,
            code: String::new(),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),
            postings: vec![
                Posting {
                    state: State::None,
                    account: String::from("Assets:Brokerage"),
                    quantity: Some(quantity!(1, "X")),
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
                Posting {
                    state: State::None,
                    account: String::from("Assets:Checking"),
                    quantity: Some(quantity!(-1, "Y")),
                    uprice: None,
                    lot_uprice: None,
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed, expected);

        let expected = journal::Xact {
            state: State::Cleared,
            code: String::from(""),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),
            postings: vec![
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Assets:Brokerage"),
                    quantity: quantity!(1, "X"),
                    uprice: quantity!(1, "Y"), // Y is the primary commodity
                    lot_uprice: LotPrice {
                        price: quantity!(1, "Y"),
                        ptype: PriceType::Floating,
                    },

                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
                journal::Posting {
                    date: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                    state: State::None,
                    acc_name: AccName::from("Assets:Checking"),
                    quantity: quantity!(-1, "Y"),
                    uprice: quantity!(1, "Y"),
                    lot_uprice: LotPrice {
                        price: quantity!(1, "Y"),
                        ptype: PriceType::Floating,
                    },
                    lot_date: None,
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
                },
            ],
        };

        assert_eq!(parsed.into_xact()?, expected);

        Ok(())
    }

    #[test]
    fn test_parse_xact6() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * Checking balance
    Assets:Brokerage      1 X
    Assets:Checking       1 Y
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;
        let xact = parsed.into_xact();
        assert!(matches!(xact, Err(ParseError::XactNoBalanced)));
        Ok(())
    }

    #[test]
    fn test_parse_xact7() -> Result<(), ParseError> {
        let xact = "\
2004/05/11 * Checking balance
    Assets:Brokerage      1 X
    Assets:Checking       1 X
";
        let mut raw_xact = match LedgerParser::parse(Rule::xact, &xact) {
            Ok(pairs) => pairs,
            Err(err) => return Err(ParseError::Parser(err)),
        };

        let parsed = parse_xact(raw_xact.next().unwrap())?;
        let xact = parsed.into_xact();
        assert!(matches!(xact, Err(ParseError::XactNoBalanced)));
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
            code: String::from("#1985"),
            date: XactDate {
                txdate: NaiveDate::from_ymd_opt(2004, 5, 11).unwrap(),
                efdate: None,
            },
            payee: String::from("Checking balance"),
            comment: String::new(),
            tags: Vec::new(),
            vtags: HashMap::new(),

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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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
                    lot_note: String::new(),
                    comment: String::new(),
                    tags: Vec::new(),
                    vtags: HashMap::new(),
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

    #[test]
    fn test_basic_tags() {
        let text = "This is a test :foo: and :bar:.";
        let tags = parse_tags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("foo"), Tag::new("bar")]);

        let text = "No tags here!";
        let tags = parse_tags(text).unwrap();
        assert!(tags.is_empty());
    }

    #[test]

    fn test_adjacent_tags() {
        let text = "This :a:b:c: is a test.";
        let tags = parse_tags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);

        let text = "This :a::b:c: is a test.";
        let tags = parse_tags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);

        let text = "This :a:b::c: is a test.";
        let tags = parse_tags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);

        let text = "This :a::b::c: is a test.";
        let tags = parse_tags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);

        let text = "This :a:b:c: is a and a TagValue: Some values";
        let tags = parse_tags(text).unwrap();
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);
        assert_eq!(
            vtag,
            Some((Tag::new("TagValue"), String::from("Some values")))
        );

        let text = "This :a::b:c: is a and a TagValue: Some values";
        let tags = parse_tags(text).unwrap();
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);
        assert_eq!(
            vtag,
            Some((Tag::new("TagValue"), String::from("Some values")))
        );

        let text = "This :a:b::c: is a and a TagValue: Some values";
        let tags = parse_tags(text).unwrap();
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);
        assert_eq!(
            vtag,
            Some((Tag::new("TagValue"), String::from("Some values")))
        );

        let text = "This :a::b::c: is a and a TagValue: Some values";
        let tags = parse_tags(text).unwrap();
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(tags, vec![Tag::new("a"), Tag::new("b"), Tag::new("c")]);
        assert_eq!(
            vtag,
            Some((Tag::new("TagValue"), String::from("Some values")))
        );
    }
    #[test]
    fn test_empty_tag() {
        let text = "Text with empty tag";
        let tags = parse_tags(text).unwrap();
        assert!(tags.is_empty());

        let text = "Text with empty tag :";
        let tags = parse_tags(text).unwrap();
        assert!(tags.is_empty());

        let text = "Text with empty tag ::";
        let tags = parse_tags(text).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn test_basic_vtags() {
        let text = "test  foo: and bar";
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(vtag, Some((Tag::new("foo"), String::from("and bar"))));

        let text = "xy :loo: and bar";
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(vtag, None);

        let text = "A tets :tag:  foo: and bar";
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(vtag, Some((Tag::new("foo"), String::from("and bar"))));

        let text = "A tets :tag:  foo: and bar :xyz:";
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(vtag, Some((Tag::new("foo"), String::from("and bar :xyz:"))));

        let text = "No tags here!";
        let vtag = parse_vtags(text).unwrap();
        assert_eq!(vtag, None);
    }
}
