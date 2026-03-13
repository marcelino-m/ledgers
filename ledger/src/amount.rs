use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

use std::collections::HashMap;
use std::fmt::{self, Debug};
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use crate::balance::Valuation;
use crate::holdings::Lot;
use crate::ntypes::{Arithmetic, Valuable};
use crate::ntypes::{Basket, Zero};
use crate::quantity::Quantity;
use crate::symbol::Symbol;
use crate::tamount::TAmount;

/// An amount representing a collection of quantities of different
/// commodities.
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Amount {
    qs: HashMap<Symbol, Decimal>,
}

impl Zero for Amount {
    /// a zero mq is a mq that with no commodities
    fn is_zero(&self) -> bool {
        // safer than self.qs.is_empty() if some relevant oper
        // forbidden call remove_zeros()
        self.qs.values().all(|q| q.is_zero())
    }
}

impl Basket for Amount {
    fn iter_quantities(&self) -> impl Iterator<Item = Quantity> {
        self.qs.iter().map(|(s, q)| Quantity { q: *q, s: *s })
    }

    fn arity(&self) -> usize {
        self.qs.len()
    }
}

// TODO: remove this imple, in practice Amount is no valueable
impl Valuable for Amount {
    fn valued_in(&self, _v: Valuation) -> Amount {
        self.clone()
    }
}

impl Arithmetic for Amount {}

impl Amount {
    pub fn to_tamount(self, d: NaiveDate) -> TAmount<Self> {
        [(d, self)].into_iter().collect()
    }

    pub fn from_quantity(q: Quantity) -> Amount {
        if q.q.is_zero() {
            return Amount::default();
        }

        let mut qs = HashMap::new();
        qs.insert(q.s, q.q);

        Amount { qs }
    }
    /// If the Amount contains exactly one commodity, return it as a
    /// Quantity.
    pub fn to_quantity(&self) -> Option<Quantity> {
        if self.qs.len() != 1 {
            return None;
        }

        let (s, q) = self.qs.iter().next().unwrap();
        Some(Quantity { s: *s, q: *q })
    }

    /// remove all commodity that have zero quantity
    fn remove_zeros(&mut self) {
        self.qs.retain(|_, &mut v| v != Decimal::ZERO);
    }
}

// --- Core: the only place where add/sub logic lives ---

impl AddAssign<&Amount> for Amount {
    fn add_assign(&mut self, rhs: &Amount) {
        for (s, q) in &rhs.qs {
            *self.qs.entry(*s).or_insert(Decimal::ZERO) += q;
        }
        self.remove_zeros();
    }
}

impl AddAssign<Amount> for Amount {
    fn add_assign(&mut self, rhs: Amount) {
        *self += &rhs;
    }
}

impl SubAssign<&Amount> for Amount {
    fn sub_assign(&mut self, rhs: &Amount) {
        for (s, q) in &rhs.qs {
            *self.qs.entry(*s).or_insert(Decimal::ZERO) -= q;
        }
        self.remove_zeros();
    }
}

impl SubAssign<Amount> for Amount {
    fn sub_assign(&mut self, rhs: Amount) {
        *self -= &rhs;
    }
}

// --- Derived Add variants ---

impl Add<Amount> for Amount {
    type Output = Amount;
    fn add(mut self, rhs: Amount) -> Amount {
        self += &rhs;
        self
    }
}

impl Add<&Amount> for Amount {
    type Output = Amount;
    fn add(mut self, rhs: &Amount) -> Amount {
        self += rhs;
        self
    }
}

impl Add<Amount> for &Amount {
    type Output = Amount;
    fn add(self, rhs: Amount) -> Amount {
        self.clone() + &rhs
    }
}

impl Add<&Amount> for &Amount {
    type Output = Amount;
    fn add(self, rhs: &Amount) -> Amount {
        self.clone() + rhs
    }
}

// --- Derived Sub variants ---

impl Sub<Amount> for Amount {
    type Output = Amount;
    fn sub(mut self, rhs: Amount) -> Amount {
        self -= &rhs;
        self
    }
}

impl Sub<&Amount> for Amount {
    type Output = Amount;
    fn sub(mut self, rhs: &Amount) -> Amount {
        self -= rhs;
        self
    }
}

impl Sub<Amount> for &Amount {
    type Output = Amount;
    fn sub(self, rhs: Amount) -> Amount {
        self.clone() - &rhs
    }
}

impl Sub<&Amount> for &Amount {
    type Output = Amount;
    fn sub(self, rhs: &Amount) -> Amount {
        self.clone() - rhs
    }
}

// --- Specialized: Quantity and Lot ---

impl Add<Quantity> for Amount {
    type Output = Amount;
    fn add(mut self, rhs: Quantity) -> Amount {
        *self.qs.entry(rhs.s).or_insert(Decimal::ZERO) += rhs.q;
        self.remove_zeros();
        self
    }
}

impl Add<Lot> for Amount {
    type Output = Amount;
    fn add(self, rhs: Lot) -> Amount {
        self + rhs.qty
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

impl Sum<Lot> for Amount {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Lot>,
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
