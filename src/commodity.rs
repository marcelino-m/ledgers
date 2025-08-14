use crate::symbol::Symbol;
use core::fmt;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::fmt::Display;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity {
    // amount of this commodity
    pub q: Decimal,
    // commodity symbol
    pub s: Symbol,
}

#[derive(Default, Clone)]
pub struct Amount {
    qs: HashMap<Symbol, Decimal>,
}

impl Quantity {
    pub fn to_amount(self) -> Amount {
        Amount::from_qs(self.q, self.s)
    }
}

impl Amount {
    pub fn from_qs(q: Decimal, s: Symbol) -> Amount {
        if q == Decimal::ZERO {
            return Amount::default();
        }

        let mut qs = HashMap::new();
        qs.insert(s, q);

        Amount { qs }
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Symbol, &Decimal)> {
        self.qs.iter()
    }

    pub fn into_iter(self) -> impl Iterator<Item = Quantity> {
        self.qs.into_iter().map(|(s, q)| Quantity { q: q, s: s })
    }

    pub fn len(&self) -> usize {
        self.qs.len()
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

impl Add<Quantity> for Amount {
    type Output = Amount;
    fn add(self, rhs: Quantity) -> Self::Output {
        let mut am = self;
        *am.qs.entry(rhs.s).or_insert(Decimal::ZERO) += rhs.q;
        am.simplify();
        am
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

impl SubAssign<Quantity> for Amount {
    fn sub_assign(&mut self, rhs: Quantity) {
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

impl fmt::Debug for Amount {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:#?}", self.qs)
    }
}

impl Sum<Quantity> for Amount {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Quantity>,
    {
        iter.fold(Amount::default(), |acc, q| acc + q)
    }
}

impl Neg for Quantity {
    type Output = Quantity;
    fn neg(self) -> Self::Output {
        return Quantity {
            q: -self.q,
            s: self.s,
        };
    }
}

impl Add<Quantity> for Quantity {
    type Output = Amount;
    fn add(self, rhs: Quantity) -> Self::Output {
        if self.s == rhs.s {
            let mut dif = Amount::from_qs(self.q + rhs.q, self.s);
            dif.simplify();
            return dif;
        }

        let mut res = Amount::default();
        res.qs.insert(self.s, self.q);
        res.qs.insert(rhs.s, -rhs.q);
        res
    }
}

impl Sub<Quantity> for Quantity {
    type Output = Amount;
    fn sub(self, rhs: Quantity) -> Self::Output {
        if self.s == rhs.s {
            let mut dif = Amount::from_qs(self.q - rhs.q, self.s);
            dif.simplify();
            return dif;
        }

        let mut res = Amount::default();
        res.qs.insert(self.s, self.q);
        res.qs.insert(rhs.s, -rhs.q);
        res
    }
}

impl Div<Quantity> for Quantity {
    type Output = Quantity;
    fn div(self, rhs: Quantity) -> Self::Output {
        return Quantity {
            q: self.q / rhs.q,
            s: self.s,
        };
    }
}

impl Mul<Quantity> for Quantity {
    type Output = Quantity;
    fn mul(self, rhs: Quantity) -> Self::Output {
        return Quantity {
            q: self.q * rhs.q,
            s: self.s,
        };
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

impl Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(precision) = f.precision() {
            write!(f, "{} {:.2$}", self.s, self.q, precision)
        } else {
            write!(f, "{} {}", self.s, self.q)
        }
    }
}
