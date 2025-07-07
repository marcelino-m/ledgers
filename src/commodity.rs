use std::ops::{Div, DivAssign, Mul, MulAssign};

pub trait Amount {
    fn value_of(&self, cmty: Commodity) -> Unit;
}

#[derive(Debug)]
pub enum Commodity {
    Symbol(String),
    None,
}

#[derive(Debug)]
pub struct Unit {
    pub q: f64,
    pub s: Commodity,
}

impl Div<f64> for Unit {
    type Output = Unit;
    fn div(self, d: f64) -> Self::Output {
        return Unit {
            q: self.q / d,
            s: self.s,
        };
    }
}

impl DivAssign<f64> for Unit {
    fn div_assign(&mut self, d: f64) {
        self.q /= d;
    }
}

impl Mul<f64> for Unit {
    type Output = Unit;
    fn mul(self, m: f64) -> Self::Output {
        return Unit {
            q: self.q * m,
            s: self.s,
        };
    }
}

impl MulAssign<f64> for Unit {
    fn mul_assign(&mut self, m: f64) {
        self.q *= m;
    }
}
