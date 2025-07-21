use std::fmt;
use std::sync::Mutex;

use bimap::BiMap;
use lazy_static::lazy_static;

type Id = u32;
type Name = String;

lazy_static! {
    static ref ID_TO_SYMBOL: Mutex<BiMap<Id, Name>> = Mutex::new(BiMap::new());
    static ref NEXT_ID: Mutex<Id> = Mutex::new(0);
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct Symbol(Id);

impl Symbol {
    pub fn new(n: &str) -> Symbol {
        let mut i2s = ID_TO_SYMBOL.lock().unwrap();
        let n = String::from(n);
        if let Some(id) = i2s.get_by_right(&n) {
            return Symbol(*id);
        }

        let mut next = NEXT_ID.lock().unwrap();
        let id = *next;

        i2s.insert(id, n.clone());

        *next += 1;

        Symbol(id)
    }

    pub fn name(&self) -> String {
        let i2s = ID_TO_SYMBOL.lock().unwrap();
        let name = i2s.get_by_left(&self.0);
        let Some(name) = name else {
            return String::from("Unknow(id)");
        };

        name.clone()
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl fmt::Debug for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "({} :: {})", self.0, self.name())
    }
}
