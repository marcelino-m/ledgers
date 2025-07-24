use rust_decimal::Decimal;
use std::collections::HashMap;

use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use crate::symbol::Symbol;

#[derive(Debug, Clone, Copy)]
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

impl Quantity {
    pub fn to_amount(self) -> Amount {
        Amount::new(self.q, self.s)
    }
}

impl Sub<Quantity> for Quantity {
    type Output = Amount;
    fn sub(self, rhs: Quantity) -> Self::Output {
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

impl Div<Quantity> for Quantity {
    type Output = Quantity;
    fn div(self, rhs: Quantity) -> Self::Output {
        return Quantity {
            q: self.q / rhs.q,
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
