use rust_decimal::{Decimal, prelude::ToPrimitive};
use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};

use std::fmt::{self, Debug, Display};
use std::ops::{Add, Div, DivAssign, Mul, MulAssign, Neg, Sub};

use crate::amount::Amount;
use crate::symbol::Symbol;

/// A quantity of a specific commodity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quantity {
    /// amount of this commodity
    pub q: Decimal,
    /// commodity symbol
    pub s: Symbol,
}

impl Quantity {
    pub fn to_amount(self) -> Amount {
        Amount::from_qs(self.q, self.s)
    }

    /// Return the quantity in absolute value
    pub fn abs(self) -> Quantity {
        Quantity {
            q: self.q.abs(),
            s: self.s,
        }
    }

    pub fn to_unit(&self) -> Quantity {
        Quantity {
            q: Decimal::ONE,
            s: self.s,
        }
    }
}

impl Neg for Quantity {
    type Output = Quantity;
    fn neg(self) -> Self::Output {
        Quantity {
            q: -self.q,
            s: self.s,
        }
    }
}

impl Add<Quantity> for Quantity {
    type Output = Amount;
    fn add(self, rhs: Quantity) -> Self::Output {
        self.to_amount() + rhs.to_amount()
    }
}

impl Sub<Quantity> for Quantity {
    type Output = Amount;
    fn sub(self, rhs: Quantity) -> Self::Output {
        self.to_amount() - rhs.to_amount()
    }
}

impl Div<Quantity> for Quantity {
    type Output = Quantity;
    fn div(self, rhs: Quantity) -> Self::Output {
        Quantity {
            q: self.q / rhs.q,
            s: self.s,
        }
    }
}

impl Mul<Quantity> for Quantity {
    type Output = Quantity;
    fn mul(self, rhs: Quantity) -> Self::Output {
        Quantity {
            q: self.q * rhs.q,
            s: self.s,
        }
    }
}

impl Div<Decimal> for Quantity {
    type Output = Quantity;
    fn div(self, d: Decimal) -> Self::Output {
        Quantity {
            q: self.q / d,
            s: self.s,
        }
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
        Quantity {
            q: self.q * m,
            s: self.s,
        }
    }
}

impl MulAssign<Decimal> for Quantity {
    fn mul_assign(&mut self, m: Decimal) {
        self.q *= m;
    }
}

impl Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let pre = f.precision().map_or(numfmt::Precision::Unspecified, |p| {
            numfmt::Precision::Decimals(p.try_into().unwrap())
        });

        // TODO: [DECIMAL] this formated must be done at level of Decimal (only q),
        // at this level we must only specify how Quantity (q plus
        // s) is displayed
        let mut ff = numfmt::Formatter::new()
            .separator(',')
            .unwrap()
            .precision(pre);

        let q = ff.fmt2(self.q.to_f64().unwrap());
        write!(f, "{} {}", self.s, q)
    }
}

impl Serialize for Quantity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(1))?;

        map.serialize_entry(&self.s, &self.q)?;
        map.end()
    }
}
