use rust_decimal::Decimal;
use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

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
        Amount::from_quantity(self)
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
        let pre = f.precision().unwrap_or(3);
        let q = utils::format_decimal_manual(self.q, pre);

        if self.s.is_empty() {
            return write!(f, "{}", q);
        }

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

mod utils {
    use rust_decimal::Decimal;
    /// naive formatting. TODO: [DECIMAL] handle formatting
    pub fn format_decimal_manual(value: Decimal, precision: usize) -> String {
        let formatted = format!("{:.prec$}", value, prec = precision);

        let parts: Vec<&str> = formatted.split('.').collect();
        let integer_part = parts[0];
        let decimal_part = parts.get(1).unwrap_or(&"");

        let formatted_integer = add_thousands_separator(integer_part);

        if decimal_part.is_empty() {
            formatted_integer
        } else {
            format!("{}.{}", formatted_integer, decimal_part)
        }
    }

    fn add_thousands_separator(s: &str) -> String {
        let (sign, num) = if s.starts_with('-') {
            ("-", &s[1..])
        } else {
            ("", s)
        };

        let chars: Vec<char> = num.chars().collect();
        let mut result = String::new();

        for (i, c) in chars.iter().enumerate() {
            if i > 0 && (chars.len() - i) % 3 == 0 {
                result.push(',');
            }
            result.push(*c);
        }

        format!("{}{}", sign, result)
    }
}
