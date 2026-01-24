use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use crate::balance_view::{Value, Viter};
use crate::quantity::Quantity;
use crate::symbol::Symbol;
use crate::tamount::TAmount;

/// An amount representing a collection of quantities of different
/// commodities.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Amount {
    qs: HashMap<Symbol, Decimal>,
}

impl Viter for Amount {
    fn iter_quantities(&self) -> impl Iterator<Item = Quantity> {
        self.qs.iter().map(|(s, q)| Quantity { q: *q, s: *s })
    }

    fn arity(&self) -> usize {
        self.qs.len()
    }
}

impl Value for Amount {
    /// a zero mq is a mq that with no commodities
    fn is_zero(&self) -> bool {
        self.qs.is_empty()
    }
}

impl Amount {
    pub fn to_tamount(self, d: NaiveDate) -> TAmount {
        [(d, self)].into_iter().collect()
    }

    pub fn from_quantity(q: Quantity) -> Amount {
        if q.q == Decimal::ZERO {
            return Amount::default();
        }

        let mut qs = HashMap::new();
        qs.insert(q.s, q.q);

        Amount { qs }
    }

    /// remove all commodity that have zero quantity
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
impl Add<Amount> for &Amount {
    type Output = Amount;
    fn add(self, rhs: Amount) -> Self::Output {
        let mut r = self.clone();
        for (s, q) in rhs.qs {
            let curr = r.qs.entry(s).or_insert(Decimal::ZERO);
            *curr += q;
        }

        r.remove_zeros();
        r
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

impl AddAssign<Amount> for Amount {
    fn add_assign(&mut self, rhs: Amount) {
        for (s, q) in rhs.qs {
            let curr = self.qs.entry(s).or_insert(Decimal::ZERO);
            *curr += q;
        }
        self.remove_zeros();
    }
}

impl SubAssign<Amount> for Amount {
    fn sub_assign(&mut self, rhs: Amount) {
        for (s, q) in rhs.qs {
            let curr = self.qs.entry(s).or_insert(Decimal::ZERO);
            *curr -= q;
        }
        self.remove_zeros();
    }
}

impl SubAssign<&Amount> for Amount {
    fn sub_assign(&mut self, rhs: &Amount) {
        for (s, q) in &rhs.qs {
            let curr = self.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr -= q;
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

impl Sub<Amount> for &Amount {
    type Output = Amount;
    fn sub(self, rhs: Amount) -> Self::Output {
        let mut res = self.clone();
        for (s, q) in rhs.qs {
            let curr = res.qs.entry(s).or_insert(Decimal::ZERO);
            *curr -= q
        }

        res.remove_zeros();
        res
    }
}

impl Sub<&Amount> for Amount {
    type Output = Amount;
    fn sub(self, rhs: &Amount) -> Self::Output {
        let mut res = self.clone();
        for (s, q) in &rhs.qs {
            let curr = res.qs.entry(*s).or_insert(Decimal::ZERO);
            *curr -= q
        }

        res.remove_zeros();
        res
    }
}

impl Sub<Amount> for Amount {
    type Output = Amount;
    fn sub(mut self, rhs: Amount) -> Self::Output {
        for (s, q) in rhs.qs {
            let curr = self.qs.entry(s).or_insert(Decimal::ZERO);
            *curr -= q
        }

        self.remove_zeros();
        self
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
