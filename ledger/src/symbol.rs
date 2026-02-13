use std::fmt::{self, Debug, Display};
use std::sync::RwLock;

use lazy_static::lazy_static;
use serde::{Serialize, Serializer};

use crate::interner::Interner;

lazy_static! {
    static ref INTERNER: RwLock<Interner> = RwLock::new(Interner::with_capacity(1024));
}

type Id = usize;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct Symbol(Id);

impl Symbol {
    pub fn new(n: &str) -> Symbol {
        let mut iner = INTERNER.write().unwrap();
        let n = iner.intern(n);
        Symbol(n)
    }

    pub fn is_empty(&self) -> bool {
        self.0 == 0
    }

    fn name(&self) -> String {
        let iner = INTERNER.read().unwrap();
        iner.name(self.0).to_owned()
    }
}

impl Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({} :: {})", self.name(), self.0)
    }
}

impl Serialize for Symbol {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.name())
    }
}
