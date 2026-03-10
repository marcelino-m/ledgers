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
    pub fn from_lots(lots: impl IntoIterator<Item = Lot>) -> Self {
        Holdings {
            qs: lots.into_iter().map(|l| (l.qty.s, l)).collect(),
        }
    }

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

        let up = std::mem::take(&mut self.h_uprice);
        let rhs_up = std::mem::take(&mut rhs.h_uprice);
        self.h_uprice = (up * self.qty.q - rhs_up * rhs.qty.q) / tot;

        let up = std::mem::take(&mut self.b_uprice);
        let rhs_up = std::mem::take(&mut rhs.b_uprice);
        self.b_uprice = (up * self.qty.q - rhs_up * rhs.qty.q) / tot;

        self.qty.q = tot;
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

        self.remove_zero();
        self
    }
}

#[cfg(test)]
mod test {
    use rust_decimal::dec;

    use super::*;

    /// Constructs an `Amount` denominated in `$` with the given quantity.
    fn uprice(q: rust_decimal::Decimal) -> Amount {
        Amount::from_quantity(Quantity {
            q,
            s: Symbol::new("$"),
        })
    }

    /// Constructs a `Lot` for the given symbol and unit prices
    /// (`m` = market, `h` = historical, `b` = book).
    fn lot(
        sym: &str,
        qty: rust_decimal::Decimal,
        m: rust_decimal::Decimal,
        h: rust_decimal::Decimal,
        b: rust_decimal::Decimal,
    ) -> Lot {
        Lot {
            qty: Quantity {
                q: qty,
                s: Symbol::new(sym),
            },
            m_uprice: uprice(m),
            h_uprice: uprice(h),
            b_uprice: uprice(b),
        }
    }

    // --- Lot tests ---

    #[test]
    fn add_accumulates_qty_and_averages_prices() {
        // 10 AAPL @ $100 + 10 AAPL @ $120 = 20 AAPL @ $110
        let a = lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100));
        let b = lot("AAPL", dec!(10), dec!(120), dec!(120), dec!(120));
        let c = a + b;

        assert_eq!(c, lot("AAPL", dec!(20), dec!(110), dec!(110), dec!(110)));
    }

    #[test]
    fn add_to_zero_clears_lot() {
        // 10 AAPL + (-10 AAPL) = 0
        let a = lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100));
        let b = lot("AAPL", dec!(-10), dec!(120), dec!(120), dec!(120));
        let c = a + b;

        assert_eq!(c, lot("AAPL", dec!(0), dec!(0), dec!(0), dec!(0)));
    }

    #[test]
    fn sub_reduces_qty_and_adjusts_prices() {
        // 20 AAPL @ $110 - 10 AAPL @ $100 = 10 AAPL @ $120
        let a = lot("AAPL", dec!(20), dec!(110), dec!(110), dec!(110));
        let b = lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100));
        let c = a - b;

        assert_eq!(c, lot("AAPL", dec!(10), dec!(120), dec!(120), dec!(120)));
    }

    #[test]
    fn sub_to_zero_clears_lot() {
        // 10 AAPL - 10 AAPL = 0
        let a = lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100));
        let b = lot("AAPL", dec!(10), dec!(120), dec!(120), dec!(120));
        let c = a - b;

        assert_eq!(c, lot("AAPL", dec!(0), dec!(0), dec!(0), dec!(0)));
    }

    // --- Holdings tests ---

    #[test]
    fn holdings_add_lot_inserts_entry() {
        let h = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        let expected = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        assert_eq!(h, expected);
    }

    #[test]
    fn holdings_add_same_symbol_merges_lots() {
        // 10 AAPL @ $100 + 10 AAPL @ $120 = 20 AAPL @ $110; MSFT unchanged
        let a = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        let b = Holdings::from_lots([lot("AAPL", dec!(10), dec!(120), dec!(120), dec!(120))]);
        let c = a + b;

        let expected = Holdings::from_lots([
            lot("AAPL", dec!(20), dec!(110), dec!(110), dec!(110)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        assert_eq!(c, expected);
    }

    #[test]
    fn holdings_add_different_symbols_keeps_both() {
        let a = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        let b = Holdings::from_lots([lot("GOOG", dec!(3), dec!(150), dec!(150), dec!(150))]);
        let c = a + b;

        let expected = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
            lot("GOOG", dec!(3), dec!(150), dec!(150), dec!(150)),
        ]);
        assert_eq!(c, expected);
    }

    #[test]
    fn holdings_add_opposite_lots_removes_entry() {
        // Cancelling AAPL leaves only MSFT.
        let a = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        let b = Holdings::from_lots([lot("AAPL", dec!(-10), dec!(100), dec!(100), dec!(100))]);
        let c = a + b;

        let expected = Holdings::from_lots([lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200))]);
        assert_eq!(c, expected);
    }

    #[test]
    fn holdings_sub_reduces_quantity() {
        // Subtract 5 AAPL; MSFT unchanged.
        let a = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        let b = Holdings::from_lots([lot("AAPL", dec!(5), dec!(100), dec!(100), dec!(100))]);
        let c = a - b;

        let expected = Holdings::from_lots([
            lot("AAPL", dec!(5), dec!(100), dec!(100), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        assert_eq!(c, expected);
    }

    #[test]
    fn holdings_sum_from_lots() {
        let lots = vec![
            lot("AAPL", dec!(10), dec!(100), dec!(100), dec!(100)),
            lot("AAPL", dec!(10), dec!(120), dec!(120), dec!(120)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ];
        let h: Holdings = lots.into_iter().sum();

        let expected = Holdings::from_lots([
            lot("AAPL", dec!(20), dec!(110), dec!(110), dec!(110)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        assert_eq!(h, expected);
    }
}
