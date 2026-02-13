use std::collections::HashMap;

/// `Interner` is a structure for **string interning**.
///
/// Each string is stored only once and assigned a unique index (`usize`),
/// allowing efficient comparisons and storage using indices instead of full strings.
/// The empty string is guaranteed to always have index `0` by default.
#[derive(Default)]
pub struct Interner {
    /// Maps each string to its unique index
    map: HashMap<String, usize>,
    /// Stores interned strings in order
    vec: Vec<String>,
}

impl Interner {
    /// Creates a new `Interner` with initial capacity `cap`
    pub fn with_capacity(cap: usize) -> Interner {
        let mut interner = Interner {
            map: HashMap::with_capacity(cap),
            vec: Vec::with_capacity(cap),
        };

        interner.intern("");
        interner
    }

    /// Interns the given string `name`.
    ///
    /// Returns the existing index if the string was already interned,
    /// or inserts it and returns a new index otherwise.
    pub fn intern(&mut self, name: &str) -> usize {
        if let Some(&idx) = self.map.get(name) {
            return idx;
        }
        let idx = self.map.len();
        self.map.insert(name.to_owned(), idx);
        self.vec.push(name.to_owned());
        idx
    }

    /// Returns the string associated with the given index `idx`.
    ///
    /// # Panics
    /// Panics if the index does not exist in the interner.
    pub fn name(&self, idx: usize) -> &str {
        self.vec[idx].as_str()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_with_capacity_pre_interns_empty_string() {
        let interner = Interner::with_capacity(10);
        assert_eq!(interner.name(0), "");
    }

    #[test]
    fn test_intern_empty_string_twice_returns_same_index() {
        let mut interner = Interner::with_capacity(10);

        let idx1 = interner.intern("");
        let idx2 = interner.intern("");

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 0, "interning \"\" twice should return the same index");
    }

    #[test]
    fn test_intern_basic_functionality() {
        let mut interner = Interner::with_capacity(10);

        // Internar nuevas strings debería retornar índices únicos
        let idx1 = interner.intern("hello");
        let idx2 = interner.intern("world");
        let idx3 = interner.intern("rust");

        assert_eq!(idx1, interner.intern("hello"));
        assert_eq!(idx2, interner.intern("world"));
        assert_eq!(idx3, interner.intern("rust"));

        assert_eq!(interner.name(idx1), "hello");
        assert_eq!(interner.name(idx2), "world");
        assert_eq!(interner.name(idx3), "rust");
    }
}
