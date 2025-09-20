use std::fmt::{self, Debug, Display};
use std::sync::Mutex;

use lazy_static::lazy_static;

use crate::interner::Interner;

lazy_static! {
    static ref INTERNER: Mutex<Interner> = Mutex::new(Interner::with_capacity(1024));
}

type Id = usize;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct Tag(Id);

impl Tag {
    pub fn new(n: &str) -> Tag {
        let mut iner = INTERNER.lock().unwrap();
        let n = iner.intern(n);
        Tag(n)
    }

    fn name(&self) -> String {
        let iner = INTERNER.lock().unwrap();
        iner.name(self.0).to_owned()
    }
}

impl Display for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Debug for Tag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({} :: {})", self.0, self.name())
    }
}
