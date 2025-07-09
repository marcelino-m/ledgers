use rust_decimal::Decimal;
use std::ops::{Div, DivAssign, Mul, MulAssign};

pub trait Amount {
    fn value_of(&self, cmty: Commodity) -> Quantity;
}

#[derive(Debug, Default, PartialEq)]
pub enum Commodity {
    #[default]
    None,
    Symbol(String),
}

#[derive(Debug, Default)]
pub struct Quantity {
    pub q: Decimal,
    pub s: Commodity,
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
