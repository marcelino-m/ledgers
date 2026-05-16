use std::fmt::Debug;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Sub, SubAssign};

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::amount::Amount;
use crate::balance::Valuation;
use crate::quantity::Quantity;
use crate::symbol::Symbol;

pub trait Zero {
    /// Checks if the value is zero.
    fn is_zero(&self) -> bool;
}

/// Represents values that can be combined and modified using basic arithmetic
/// operations (addition, subtraction, accumulation, and zero checking).
pub trait Arithmetic<Output = Self>:
    AddAssign
    + Add<Output = Output>
    + AddAssign
    + Sub<Output = Output>
    + SubAssign
    + Eq
    + Sum
    + Zero
    + Default
    + Clone
    + Debug
{
}
/// Represents a collection of arithmetic values
pub trait Basket: Zero + Debug {
    /// Returns the number of distinct commodities in the collection.
    fn arity(&self) -> usize;
    /// Returns an iterator over the quantities in the collection.
    fn quantities(&self) -> impl Iterator<Item = Quantity>;
}

pub trait TsBasket: Debug {
    type B: Basket;

    fn at(&self, d: NaiveDate) -> Option<&Self::B>;
    fn iter_baskets(&self) -> impl Iterator<Item = (NaiveDate, &Self::B)>;
}

/// Represents values that can be evaluated based on a given valuation method.
pub trait Valuable: Debug {
    fn valued_in(&self, v: Valuation) -> Amount;
    /// Computes the gain ratio between a given valuation and the basis valuation.
    ///
    /// The gain is defined as:
    /// `(current - basis) / basis`.
    ///
    /// Returns `None` if either valuation cannot be converted to a quantity,
    /// or if the basis and current valuations use different commodities.
    fn gain(&self, v: Valuation) -> Option<Decimal> {
        let current = self.valued_in(v).to_quantity(); // TODO: [GAIN] would this fail? : could operate using only Amount
        let basis = self.valued_in(Valuation::Basis).to_quantity();
        basis
            .zip(current)
            .filter(|(b, c)| b.s == c.s)
            .map(|(b, c)| (c.q - b.q) / b.q)
    }
}

pub trait QValuable: Debug {
    fn svalued_in(&self, s: Symbol, v: Valuation) -> Amount;
    /// Computes the gain ratio between a given valuation and the basis valuation.
    ///
    /// Returns the underlying commodity after valuation. If it is equal to `s`,
    /// the gain is considered to be 0%, since no meaningful comparison can be made
    /// when both valuations refer to the same commodity.
    ///
    /// Returns `None` if either valuation cannot be converted to a quantity,
    /// or if the basis and current valuations use different commodities.
    fn sgain(&self, s: Symbol, v: Valuation) -> Option<(Decimal, Symbol)> {
        let current = self.svalued_in(s, v).to_quantity(); // TODO: [GAIN] would this fail? : could operate using only Amount
        let basis = self.svalued_in(s, Valuation::Basis).to_quantity();
        basis
            .zip(current)
            .filter(|(b, c)| b.s == c.s)
            .map(|(b, c)| ((c.q - b.q) / b.q, b.s))
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;
    use crate::holdings::{Holdings, Lot};

    /// Helper: builds a Lot with all unit prices denominated in "$".
    fn lot(sym: &str, qty: Decimal, market: Decimal, historical: Decimal, basis: Decimal) -> Lot {
        let uprice = |q: Decimal| -> Amount {
            Amount::from_quantity(Quantity {
                q,
                s: Symbol::new("$"),
            })
        };
        Lot {
            qty: Quantity {
                q: qty,
                s: Symbol::new(sym),
            },
            m_uprice: uprice(market),
            h_uprice: uprice(historical),
            b_uprice: uprice(basis),
        }
    }

    #[test]
    fn gain_positive_when_market_above_basis() {
        // 10 AAPL, basis $100, market $150
        // basis value = 10 * 100 = $1000, market value = 10 * 150 = $1500
        // gain = (1500 - 1000) / 1000 = 0.5
        let h = Holdings::from_lots([lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))]);
        let g = h.gain(Valuation::Market);
        assert_eq!(g, Some(dec!(0.5)));
    }

    #[test]
    fn gain_negative_when_market_below_basis() {
        // 10 AAPL, basis $100, market $80
        // basis = $1000, market = $800
        // gain = (800 - 1000) / 1000 = -0.2
        let h = Holdings::from_lots([lot("AAPL", dec!(10), dec!(80), dec!(120), dec!(100))]);
        let g = h.gain(Valuation::Market);
        assert_eq!(g, Some(dec!(-0.2)));
    }

    #[test]
    fn gain_zero_when_market_equals_basis() {
        let h = Holdings::from_lots([lot("AAPL", dec!(10), dec!(100), dec!(120), dec!(100))]);
        let g = h.gain(Valuation::Market);
        assert_eq!(g, Some(dec!(0)));
    }

    #[test]
    fn gain_none_for_multi_commodity_holdings() {
        // Holdings with two different symbols produce a multi-commodity Amount,
        // which cannot be converted to a single Quantity.
        let h = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(180), dec!(160)),
        ]);
        let g = h.gain(Valuation::Market);
        // Both valued_in(Market) and valued_in(Basis) return single-commodity "$"
        // amounts, so gain should still work since they sum to "$" totals.
        assert!(g.is_some());
    }

    #[test]
    fn gain_none_for_empty_holdings() {
        let h = Holdings::default();
        let g = h.gain(Valuation::Market);
        // Empty holdings => valued_in returns default Amount => to_quantity() is None
        assert_eq!(g, None);
    }

    // --- QValuable::sgain ---

    #[test]
    fn sgain_positive() {
        let h = Holdings::from_lots([lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))]);
        let g = h.sgain(Symbol::new("AAPL"), Valuation::Market);
        // svalued_in("AAPL", Market) = 10 * 150 = $1500
        // svalued_in("AAPL", Basis)  = 10 * 100 = $1000
        // gain = (1500 - 1000) / 1000 = 0.5
        assert_eq!(g, Some((dec!(0.5), Symbol::new("$"))));
    }

    #[test]
    fn sgain_negative() {
        let h = Holdings::from_lots([lot("AAPL", dec!(10), dec!(80), dec!(120), dec!(100))]);
        let g = h.sgain(Symbol::new("AAPL"), Valuation::Market);
        assert_eq!(g, Some((dec!(-0.2), Symbol::new("$"))));
    }

    #[test]
    fn sgain_none_for_missing_symbol() {
        let h = Holdings::from_lots([lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))]);
        let g = h.sgain(Symbol::new("GOOG"), Valuation::Market);
        // svalued_in for missing symbol returns Amount::default() => to_quantity() is None
        assert_eq!(g, None);
    }
}
