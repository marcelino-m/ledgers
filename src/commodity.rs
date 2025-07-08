use std::ops::{Div, DivAssign, Mul, MulAssign};

pub trait Amount {
    fn value_of(&self, cmty: Commodity) -> Quantity;
}

#[derive(Debug)]
pub enum Commodity {
    Symbol(String),
    None,
}

#[derive(Debug)]
pub struct Quantity {
    pub q: f64,
    pub s: Commodity,
}

impl Div<f64> for Quantity {
    type Output = Quantity;
    fn div(self, d: f64) -> Self::Output {
        return Quantity {
            q: self.q / d,
            s: self.s,
        };
    }
}

impl DivAssign<f64> for Quantity {
    fn div_assign(&mut self, d: f64) {
        self.q /= d;
    }
}

impl Mul<f64> for Quantity {
    type Output = Quantity;
    fn mul(self, m: f64) -> Self::Output {
        return Quantity {
            q: self.q * m,
            s: self.s,
        };
    }
}

impl MulAssign<f64> for Quantity {
    fn mul_assign(&mut self, m: f64) {
        self.q *= m;
    }
}
