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
    fn iter_quantities(&self) -> impl Iterator<Item = Quantity>;
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
        let current = self.valued_in(v).to_quantity();
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
        let current = self.svalued_in(s, v).to_quantity();
        let basis = self.svalued_in(s, Valuation::Basis).to_quantity();
        basis
            .zip(current)
            .filter(|(b, c)| b.s == c.s)
            .map(|(b, c)| ((c.q - b.q) / b.q, b.s))
    }
}
