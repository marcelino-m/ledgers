use std::collections::HashMap;

use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign};

use rust_decimal::Decimal;
use serde::Serialize;

use crate::amount::Amount;
use crate::balance::Valuation;
use crate::ntypes::{Arithmetic, Basket, QValuable, Valuable, Zero};
use crate::quantity::Quantity;
use crate::symbol::Symbol;

/// Represents the unit price for each unit of `n`.
#[derive(Clone, PartialEq, Eq, Debug, Serialize)]
pub struct Lot {
    /// Quantity of the commodity
    pub qty: Quantity,
    /// Unit price for each unit of `n`. depending of the valuation
    /// method it can be the base (book value), market price, or
    /// historical price.
    pub m_uprice: Amount,
    /// Unit price based on the moment of the transaction
    pub h_uprice: Amount,
    /// Book unit price
    pub b_uprice: Amount,
}

impl Lot {
    fn make_zero(&mut self) {
        self.qty.q = Decimal::ZERO;
        self.m_uprice = Amount::default();
        self.h_uprice = Amount::default();
        self.b_uprice = Amount::default();
    }
}

impl Valuable for Lot {
    fn valued_in(&self, v: Valuation) -> Amount {
        let q = self.qty.q;
        match v {
            Valuation::Quantity => self.qty.to_amount(),
            Valuation::Market => self.m_uprice.clone() * q,
            Valuation::Historical => self.h_uprice.clone() * q,
            Valuation::Basis => self.b_uprice.clone() * q,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Default, Debug, Serialize)]
pub struct Holdings {
    qs: HashMap<Symbol, Lot>,
}

impl Holdings {
    fn remove_zero(&mut self) {
        self.qs.retain(|_, l| !l.qty.q.is_zero());
    }
}

impl Zero for Holdings {
    fn is_zero(&self) -> bool {
        self.qs.values().all(|l| l.qty.q.is_zero())
    }
}

impl Basket for Holdings {
    fn iter_quantities(&self) -> impl Iterator<Item = Quantity> {
        self.qs.iter().map(|(_, l)| l.qty)
    }

    fn arity(&self) -> usize {
        self.qs.len()
    }
}

impl Valuable for Holdings {
    fn valued_in(&self, v: Valuation) -> Amount {
        let mut res = Amount::default();
        for l in self.qs.values() {
            res += l.valued_in(v);
        }
        res
    }
}

impl QValuable for Holdings {
    fn svalued_in(&self, s: Symbol, v: Valuation) -> Amount {
        if let Some(l) = self.qs.get(&s) {
            l.valued_in(v)
        } else {
            Amount::default()
        }
    }
}

impl Arithmetic for Holdings {}

impl Add for Lot {
    type Output = Lot;

    fn add(mut self, rhs: Lot) -> Lot {
        self += rhs;
        self
    }
}

impl AddAssign for Lot {
    fn add_assign(&mut self, mut rhs: Lot) {
        let tot = self.qty.q + rhs.qty.q;
        if tot.is_zero() {
            self.make_zero();
            return;
        }

        let up = std::mem::take(&mut self.m_uprice);
        let rhs_up = std::mem::take(&mut rhs.m_uprice);
        self.m_uprice = (up * self.qty.q + rhs_up * rhs.qty.q) / tot;

        let up = std::mem::take(&mut self.h_uprice);
        let rhs_up = std::mem::take(&mut rhs.h_uprice);
        self.h_uprice = (up * self.qty.q + rhs_up * rhs.qty.q) / tot;

        let up = std::mem::take(&mut self.b_uprice);
        let rhs_up = std::mem::take(&mut rhs.b_uprice);
        self.b_uprice = (up * self.qty.q + rhs_up * rhs.qty.q) / tot;

        self.qty.q = tot
    }
}

impl Sub for Lot {
    type Output = Lot;

    fn sub(mut self, rhs: Lot) -> Lot {
        self -= rhs;
        self
    }
}

impl SubAssign for Lot {
    fn sub_assign(&mut self, mut rhs: Lot) {
        let tot = self.qty.q - rhs.qty.q;
        if tot.is_zero() {
            self.make_zero();
            return;
        }

        let up = std::mem::take(&mut self.m_uprice);
        let rhs_up = std::mem::take(&mut rhs.m_uprice);
        self.m_uprice = (up * self.qty.q - rhs_up * rhs.qty.q) / tot;
        self.qty.q = tot;

        let up = std::mem::take(&mut self.h_uprice);
        let rhs_up = std::mem::take(&mut rhs.h_uprice);
        self.h_uprice = (up * self.qty.q - rhs_up * rhs.qty.q) / tot;

        let up = std::mem::take(&mut self.b_uprice);
        let rhs_up = std::mem::take(&mut rhs.b_uprice);
        self.b_uprice = (up * self.qty.q - rhs_up * rhs.qty.q) / tot;
    }
}

impl Add<Holdings> for Holdings {
    type Output = Holdings;
    fn add(mut self, rhs: Holdings) -> Self::Output {
        self += rhs;
        self
    }
}

impl AddAssign<Holdings> for Holdings {
    fn add_assign(&mut self, rhs: Holdings) {
        for (s, rhs_dv) in rhs.qs {
            self.qs
                .entry(s)
                .and_modify(|v| *v += rhs_dv.clone())
                .or_insert(rhs_dv);
        }

        self.remove_zero();
    }
}

impl Sub<Holdings> for Holdings {
    type Output = Holdings;
    fn sub(mut self, rhs: Holdings) -> Self::Output {
        self -= rhs;
        self
    }
}

impl SubAssign<Holdings> for Holdings {
    fn sub_assign(&mut self, rhs: Holdings) {
        for (s, rhs_dv) in rhs.qs {
            self.qs.entry(s).and_modify(|v| *v -= rhs_dv);
        }
        self.remove_zero();
    }
}

impl Sum<Holdings> for Holdings {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Holdings>,
    {
        iter.fold(Holdings::default(), |acc, q| acc + q)
    }
}

impl Sum<Lot> for Holdings {
    fn sum<I>(iter: I) -> Self
    where
        I: Iterator<Item = Lot>,
    {
        iter.fold(Holdings::default(), |acc, q| acc + q)
    }
}

impl Add<Lot> for Holdings {
    type Output = Holdings;
    fn add(mut self, mut rhs: Lot) -> Self::Output {
        self.qs
            .entry(rhs.qty.s)
            .and_modify(|l| {
                let tot = l.qty.q + rhs.qty.q;
                if tot.is_zero() {
                    l.qty.q = Decimal::ZERO;
                    l.m_uprice = Amount::default();
                    l.b_uprice = Amount::default();
                    l.h_uprice = Amount::default();
                    return;
                }

                let up = std::mem::take(&mut l.m_uprice);
                let rhs_up = std::mem::take(&mut rhs.m_uprice);
                l.m_uprice = (up * l.qty.q + rhs_up * rhs.qty.q) / tot;

                let up = std::mem::take(&mut l.b_uprice);
                let rhs_up = std::mem::take(&mut rhs.b_uprice);
                l.b_uprice = (up * l.qty.q + rhs_up * rhs.qty.q) / tot;

                let up = std::mem::take(&mut l.h_uprice);
                let rhs_up = std::mem::take(&mut rhs.h_uprice);
                l.h_uprice = (up * l.qty.q + rhs_up * rhs.qty.q) / tot;

                l.qty.q = tot;
            })
            .or_insert(rhs);
        self
    }
}
