use std::fmt::{self, Debug, Display};
use std::sync::RwLock;

use lazy_static::lazy_static;

use crate::interner::Interner;

lazy_static! {
    static ref INTERNER: RwLock<Interner> = RwLock::new(Interner::with_capacity(1024));
}

type Id = usize;

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct Tag(Id);

impl Tag {
    pub fn new(n: &str) -> Tag {
        let mut iner = INTERNER.write().unwrap();
        let n = iner.intern(n);
        Tag(n)
    }

    fn name(&self) -> String {
        let iner = INTERNER.read().unwrap();
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn tag_name_returns_original_string() {
        let t = Tag::new("payee");
        assert_eq!(t.name(), "payee");
    }

    #[test]
    fn tag_name_with_spaces() {
        let t = Tag::new("my tag");
        assert_eq!(t.name(), "my tag");
    }

    #[test]
    fn tag_name_empty() {
        let t = Tag::new("");
        assert_eq!(t.name(), "");
    }

    #[test]
    fn same_name_same_tag() {
        let a = Tag::new("payee");
        let b = Tag::new("payee");
        assert_eq!(a, b);
    }

    #[test]
    fn different_name_different_tag() {
        let a = Tag::new("payee");
        let b = Tag::new("note");
        assert_ne!(a, b);
    }
}
