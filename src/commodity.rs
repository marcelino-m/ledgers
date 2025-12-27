use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use std::collections::HashMap;
use std::fmt::{self, Debug, Display};
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::symbol::Symbol;

/// Specifies the method to calculate the commodity price
/// value.
///
/// # Variants
///
/// - `Basis`: Calculate using the book value
/// - `Quantity`: Calculate based on raw quantities without valuation.
/// - `Market`: Calculate using the most recent market value from the price database.
#[derive(Debug, Copy, Clone)]
pub enum Valuation {
    Basis,
    Quantity,
    Market,
    Historical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity {
    // amount of this commodity
    pub q: Decimal,
    // commodity symbol
    pub s: Symbol,
}

#[derive(Default, Clone, PartialEq, Eq)]
pub struct Amount {
    qs: HashMap<Symbol, Decimal>,
}

impl Quantity {
    pub fn to_amount(self) -> Amount {
        Amount::from_qs(self.q, self.s)
    }

    /// Return the quantity in absolute value
    pub fn abs(self) -> Quantity {
        return Quantity {
            q: self.q.abs(),
            s: self.s,
        };
    }
}

impl Amount {
    /// Make a new empty Amount
    pub fn new() -> Amount {
        Amount::default()
    }

    pub fn iter_quantities(&self) -> impl Iterator<Item = Quantity> {
        self.qs.iter().map(|(s, q)| Quantity { q: *q, s: *s })
    }

    pub fn len(&self) -> usize {
        self.qs.len()
    }

    // a zero mq is a mq that with no commodities
    pub fn is_zero(&self) -> bool {
        self.qs.len() == 0
    }

    fn from_qs(q: Decimal, s: Symbol) -> Amount {
        if q == Decimal::ZERO {
            return Amount::default();
        }

        let mut qs = HashMap::new();
        qs.insert(s, q);

        Amount { qs }
    }

    // remove all commodity that have zero quantity
    fn remove_zeros(&mut self) {
        self.qs.retain(|_, &mut v| v != Decimal::ZERO);
    }
}

impl Add<Amount> for Amount {
    type Output = Amount;
    fn add(self, rhs: Amount) -> Self::Output {
        let mut res = self;
        for (s, q) in rhs.qs.iter() {
            let curr = res.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr += *q;
        }

        res.remove_zeros();
        res
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

        res.remove_zeros();
        res
    }
}

impl Add<&Amount> for Amount {
    type Output = Amount;
    fn add(mut self, rhs: &Amount) -> Self::Output {
        for (s, q) in rhs.qs.iter() {
            let curr = self.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr += *q;
        }

        self.remove_zeros();
        self
    }
}

impl Add<Quantity> for Amount {
    type Output = Amount;
    fn add(self, rhs: Quantity) -> Self::Output {
        let mut am = self;
        *am.qs.entry(rhs.s).or_insert(Decimal::ZERO) += rhs.q;
        am.remove_zeros();
        am
    }
}

impl AddAssign<&Amount> for Amount {
    fn add_assign(&mut self, rhs: &Amount) {
        for (s, q) in rhs.qs.iter() {
            let curr = self.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr += *q;
        }
        self.remove_zeros();
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

        res.remove_zeros();
        res
    }
}

impl AddAssign<Quantity> for Amount {
    fn add_assign(&mut self, rhs: Quantity) {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) += rhs.q;
        self.remove_zeros();
    }
}

impl SubAssign<Quantity> for Amount {
    fn sub_assign(&mut self, rhs: Quantity) {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) -= rhs.q;
        self.remove_zeros();
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

impl Debug for Amount {
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

impl Sum<Amount> for Amount {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Amount>,
    {
        iter.fold(Amount::default(), |acc, q| acc + q)
    }
}

impl<'a> Sum<&'a Amount> for Amount {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = &'a Amount>,
    {
        iter.fold(Amount::default(), |acc, q| acc + q)
    }
}

impl Serialize for Amount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.qs.len()))?;
        for (k, v) in &self.qs {
            map.serialize_entry(k, v)?;
        }
        map.end()
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
            dif.remove_zeros();
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
            dif.remove_zeros();
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
        let pre = f.precision().map_or(numfmt::Precision::Unspecified, |p| {
            numfmt::Precision::Decimals(p.try_into().unwrap())
        });

        // TODO: [DECIMAL] this formated must be done at level of Decimal (only q),
        // at this level we must only specify how Quantity (q plus
        // s) is displayed
        let mut ff = numfmt::Formatter::new()
            .separator(',')
            .unwrap()
            .precision(pre);

        let q = ff.fmt2(self.q.to_f64().unwrap());
        write!(f, "{} {}", self.s, q)
    }
}

impl Serialize for Quantity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;

        map.serialize_entry(&self.s, &self.q)?;
        map.end()
    }
}
