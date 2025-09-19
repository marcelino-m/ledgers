use std::collections::HashMap;

/// `Interner` is a structure for **string interning**.
///
/// Each string is stored only once and assigned a unique index (`usize`),
/// allowing efficient comparisons and storage using indices instead of full strings.
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
        Interner {
            map: HashMap::with_capacity(cap),
            vec: Vec::with_capacity(cap),
        }
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
