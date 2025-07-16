use bimap::BiMap;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use lazy_static::lazy_static;
use std::sync::Mutex;

type Id = u32;
type Name = String;

lazy_static! {
    static ref ID_TO_SYMBOL: Mutex<BiMap<Id, Name>> = Mutex::new(BiMap::new());
    static ref NEXT_ID: Mutex<Id> = Mutex::new(0);
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct Symbol(Id);

#[derive(Debug)]
pub struct Quantity {
    // amount of this commodity
    pub q: Decimal,
    // commodity symbol
    pub s: Symbol,
}

#[derive(Debug, Default, Clone)]
pub struct Amount {
    qs: HashMap<Symbol, Decimal>,
}

impl Symbol {
    pub fn new(n: &str) -> Symbol {
        let mut i2s = ID_TO_SYMBOL.lock().unwrap();
        let n = String::from(n);
        if let Some(id) = i2s.get_by_right(&n) {
            return Symbol(*id);
        }

        let mut next = NEXT_ID.lock().unwrap();
        let id = *next;

        i2s.insert(id, n.clone());

        *next += 1;

        Symbol(id)
    }

    pub fn name(id: u32) -> String {
        let i2s = ID_TO_SYMBOL.lock().unwrap();
        let name = i2s.get_by_left(&id);
        let Some(name) = name else {
            return String::from("Unknow(id)");
        };

        name.clone()
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", Symbol::name(self.0))
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}::{}", self.0, Symbol::name(self.0))
    }
}

impl Quantity {
    pub fn to_amount(self) -> Amount {
        Amount::new(self.q, self.s)
    }
}

impl Sub<&Quantity> for &Quantity {
    type Output = Amount;
    fn sub(self, rhs: &Quantity) -> Self::Output {
        if self.s == rhs.s {
            let mut dif = Amount::new(self.q - rhs.q, self.s);
            dif.simplify();
            return dif;
        }

        let mut res = Amount::default();
        res.qs.insert(self.s, self.q);
        res.qs.insert(rhs.s, -rhs.q);
        res
    }
}

impl Div<Decimal> for Quantity {
    type Output = Quantity;
    fn div(self, d: Decimal) -> Self::Output {
        return Quantity {
            q: self.q / d,
            s: self.s,
        };
    }
}

impl DivAssign<Decimal> for Quantity {
    fn div_assign(&mut self, d: Decimal) {
        self.q /= d;
    }
}

impl Mul<Decimal> for Quantity {
    type Output = Quantity;
    fn mul(self, m: Decimal) -> Self::Output {
        return Quantity {
            q: self.q * m,
            s: self.s,
        };
    }
}

impl MulAssign<Decimal> for Quantity {
    fn mul_assign(&mut self, m: Decimal) {
        self.q *= m;
    }
}

impl Amount {
    pub fn new(q: Decimal, s: Symbol) -> Amount {
        if q == Decimal::ZERO {
            return Amount::default();
        }

        let mut qs = HashMap::new();
        qs.insert(s, q);

        Amount { qs }
    }

    // a zero mq is a mq that with no commodities
    pub fn is_zero(&self) -> bool {
        self.qs.len() == 0
    }

    // remove all commodity that have zero quantity
    pub fn simplify(&mut self) {
        self.qs.retain(|_, &mut v| v != Decimal::ZERO);
    }
}

impl Add<&Amount> for &Amount {
    type Output = Amount;
    fn add(self, rhs: &Amount) -> Self::Output {
        let mut res = self.clone();

        for (s, q) in rhs.qs.iter() {
            let curr = res.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr += *q;
        }

        res.simplify();
        res
    }
}

impl AddAssign<&Amount> for Amount {
    fn add_assign(&mut self, rhs: &Amount) {
        for (s, q) in rhs.qs.iter() {
            let curr = self.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr += *q;
        }
        self.simplify();
    }
}

impl Sub<&Amount> for &Amount {
    type Output = Amount;
    fn sub(self, rhs: &Amount) -> Self::Output {
        let mut res = self.clone();

        for (s, q) in rhs.qs.iter() {
            let curr = res.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr -= *q
        }

        res.simplify();
        res
    }
}

impl AddAssign<Quantity> for Amount {
    fn add_assign(&mut self, rhs: Quantity) {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) += rhs.q;
        self.simplify();
    }
}

impl AddAssign<&Quantity> for Amount {
    fn add_assign(&mut self, rhs: &Quantity) {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) += rhs.q;
        self.simplify();
    }
}

impl SubAssign<Quantity> for Amount {
    fn sub_assign(&mut self, rhs: Quantity) {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) -= rhs.q;
        self.simplify();
    }
}

impl SubAssign<&Quantity> for Amount {
    fn sub_assign(&mut self, rhs: &Quantity) {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) -= rhs.q;
        self.simplify();
    }
}

impl Div<Decimal> for Amount {
    type Output = Amount;
    fn div(mut self, d: Decimal) -> Self::Output {
        for mut val in self.qs.values_mut() {
            val /= d
        }
        self
    }
}

impl Mul<Decimal> for Amount {
    type Output = Amount;
    fn mul(mut self, m: Decimal) -> Self::Output {
        for mut val in self.qs.values_mut() {
            val *= m
        }
        self
    }
}

impl DivAssign<Decimal> for Amount {
    fn div_assign(&mut self, d: Decimal) {
        for mut val in self.qs.values_mut() {
            val /= d
        }
    }
}

impl MulAssign<Decimal> for Amount {
    fn mul_assign(&mut self, m: Decimal) {
        for mut val in self.qs.values_mut() {
            val *= m
        }
    }
}
