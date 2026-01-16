use chrono::NaiveDate;
use serde::Serialize;
use std::collections::BTreeMap;
use std::iter::Sum;

use std::ops::{Add, AddAssign, Sub, SubAssign};

use crate::balance_view::{TValue, Value};
use crate::commodity::Amount;

/// An amount in different timestamps
#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct TAmount {
    pub ts: BTreeMap<NaiveDate, Amount>,
}

impl Value for TAmount {
    fn new() -> Self {
        TAmount::default()
    }

    fn is_zero(&self) -> bool {
        self.ts.values().all(|m| m.is_zero())
    }
}

impl TValue for TAmount {
    fn iter(&self) -> impl Iterator<Item = (NaiveDate, &Amount)> {
        self.ts.iter().map(|(d, m)| (*d, m))
    }
}

impl IntoIterator for TAmount {
    type Item = (NaiveDate, Amount);
    type IntoIter = std::collections::btree_map::IntoIter<NaiveDate, Amount>;

    fn into_iter(self) -> Self::IntoIter {
        self.ts.into_iter()
    }
}

impl<'a> IntoIterator for &'a TAmount {
    type Item = (&'a NaiveDate, &'a Amount);
    type IntoIter = std::collections::btree_map::Iter<'a, NaiveDate, Amount>;

    fn into_iter(self) -> Self::IntoIter {
        self.ts.iter()
    }
}

impl FromIterator<(NaiveDate, Amount)> for TAmount {
    fn from_iter<T: IntoIterator<Item = (NaiveDate, Amount)>>(iter: T) -> Self {
        Self {
            ts: iter.into_iter().collect(),
        }
    }
}

impl Add<TAmount> for TAmount {
    type Output = TAmount;
    fn add(mut self, rhs: TAmount) -> Self::Output {
        rhs.ts.into_iter().for_each(|(t, m)| {
            *self.ts.entry(t).or_default() += m;
        });
        self
    }
}

impl AddAssign<TAmount> for TAmount {
    fn add_assign(&mut self, rhs: TAmount) {
        rhs.ts.into_iter().for_each(|(t, m)| {
            *self.ts.entry(t).or_default() += m;
        });
    }
}

impl Sub<TAmount> for TAmount {
    type Output = TAmount;
    fn sub(mut self, rhs: TAmount) -> Self::Output {
        rhs.ts.into_iter().for_each(|(t, m)| {
            *self.ts.entry(t).or_default() -= m;
        });
        self
    }
}

impl SubAssign<TAmount> for TAmount {
    fn sub_assign(&mut self, rhs: TAmount) {
        rhs.ts.into_iter().for_each(|(t, m)| {
            *self.ts.entry(t).or_default() -= m;
        });
    }
}

impl Sum<TAmount> for TAmount {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = TAmount>,
    {
        iter.fold(TAmount::new(), |acc, q| acc + q)
    }
}
