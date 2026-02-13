use chrono::NaiveDate;
use serde::Serialize;
use std::collections::BTreeMap;
use std::iter::Sum;

use std::ops::{Add, AddAssign, Sub, SubAssign};

use crate::ntypes::{Arithmetic, Basket, TsBasket, Zero};

/// An amount in different timestamps
#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct TAmount<V>
where
    V: Arithmetic + Basket,
{
    pub ts: BTreeMap<NaiveDate, V>,
}

impl<V> Arithmetic for TAmount<V> where V: Arithmetic + Basket {}

impl<V> Zero for TAmount<V>
where
    V: Arithmetic + Basket,
{
    fn is_zero(&self) -> bool {
        self.ts.values().all(|v| v.is_zero())
    }
}

impl<V> TsBasket for TAmount<V>
where
    V: Arithmetic + Basket,
{
    type B = V;

    fn at(&self, d: NaiveDate) -> Option<&Self::B> {
        self.ts.get(&d)
    }

    fn iter_baskets(&self) -> impl Iterator<Item = (NaiveDate, &Self::B)> {
        self.ts.iter().map(|(d, m)| (*d, m))
    }
}

impl<V> IntoIterator for TAmount<V>
where
    V: Arithmetic + Basket,
{
    type Item = (NaiveDate, V);
    type IntoIter = std::collections::btree_map::IntoIter<NaiveDate, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.ts.into_iter()
    }
}

impl<'a, V: Arithmetic + Basket> IntoIterator for &'a TAmount<V> {
    type Item = (&'a NaiveDate, &'a V);
    type IntoIter = std::collections::btree_map::Iter<'a, NaiveDate, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.ts.iter()
    }
}

impl<V: Basket + Arithmetic> FromIterator<(NaiveDate, V)> for TAmount<V> {
    fn from_iter<T: IntoIterator<Item = (NaiveDate, V)>>(iter: T) -> Self {
        Self {
            ts: iter.into_iter().collect(),
        }
    }
}

impl<V> Add<TAmount<V>> for TAmount<V>
where
    V: Basket + Arithmetic,
{
    type Output = TAmount<V>;
    fn add(mut self, rhs: TAmount<V>) -> Self::Output {
        self += rhs;
        self
    }
}

impl<V> AddAssign<TAmount<V>> for TAmount<V>
where
    V: Basket + Arithmetic,
{
    fn add_assign(&mut self, rhs: TAmount<V>) {
        rhs.ts.into_iter().for_each(|(t, m)| {
            *self.ts.entry(t).or_default() += m;
        });
    }
}

impl<V> Sub<TAmount<V>> for TAmount<V>
where
    V: Basket + Arithmetic,
{
    type Output = TAmount<V>;
    fn sub(mut self, rhs: TAmount<V>) -> Self::Output {
        self -= rhs;
        self
    }
}

impl<V> SubAssign<TAmount<V>> for TAmount<V>
where
    V: Basket + Arithmetic,
{
    fn sub_assign(&mut self, rhs: TAmount<V>) {
        rhs.ts.into_iter().for_each(|(t, m)| {
            *self.ts.entry(t).or_default() -= m;
        });
    }
}

impl<V> Sum<TAmount<V>> for TAmount<V>
where
    V: Basket + Arithmetic,
{
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = TAmount<V>>,
    {
        iter.fold(TAmount::default(), |acc, q| acc + q)
    }
}
