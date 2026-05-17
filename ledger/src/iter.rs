use std::{self, collections::VecDeque};

/// A wrapper around an iterator that allows multiple consecutive
/// peeks.  Each call to `peek()` advances to the next element, and
/// `unpeek()` can undo previous peeks.
pub struct MultiPeek<I: Iterator> {
    iter: I,
    cur: usize,
    peeked: VecDeque<I::Item>,
}

impl<I: Iterator> MultiPeek<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            cur: 0,
            peeked: VecDeque::with_capacity(50),
        }
    }

    /// Consumes/Discard all peeked elements, resetting the peek
    pub fn consume_peeked(&mut self) {
        self.cur = 0;
        self.peeked.clear();
    }

    /// Peeks at the next element, advancing the peek position.  Each
    /// consecutive call returns the next element in sequence.
    pub fn peek(&mut self) -> Option<&I::Item> {
        if self.cur < self.peeked.len() {
            self.cur += 1;
            return self.peeked.get(self.cur - 1);
        }

        if let Some(item) = self.iter.next() {
            self.peeked.push_back(item);
            self.cur += 1;
            return self.peeked.back();
        }

        None
    }

    /// Moves the peek position back by one
    pub fn unpeek(&mut self) {
        if self.cur > 0 {
            self.cur -= 1;
        }
    }

    /// Moves the peek position back by one
    pub fn peek_reset(&mut self) -> &mut Self {
        self.cur = 0;
        self
    }
}

impl<I: Iterator> Iterator for MultiPeek<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.cur = 0;
        if let Some(item) = self.peeked.pop_front() {
            Some(item)
        } else {
            self.iter.next()
        }
    }
}

/// An iterator adapter that yields `(current, Option<next>)` pairs.
///
/// Each item is paired with the following item in the sequence. For
/// the last element, `next` is `None`.
///
/// # Example
///
/// ```
/// use ledger::iter::WithNext;
///
/// let v = vec![1, 2, 3];
/// let pairs: Vec<_> = WithNext::new(v.into_iter()).collect();
/// assert_eq!(pairs, vec![(1, Some(2)), (2, Some(3)), (3, None)]);
/// ```
pub struct WithNext<I: Iterator> {
    iter: std::iter::Peekable<I>,
}

impl<I: Iterator> WithNext<I> {
    pub fn new(iter: I) -> Self {
        Self { iter: iter.peekable() }
    }
}

impl<I: Iterator> Iterator for WithNext<I>
where
    I::Item: Clone,
{
    type Item = (I::Item, Option<I::Item>);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.iter.next()?;
        let next = self.iter.peek().cloned();
        Some((current, next))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_peek_and_unpeek() {
        let data = vec![1, 2, 3, 4, 5];
        let mut peekable = MultiPeek::new(data.into_iter());

        // Test consecutive peeks advance
        assert_eq!(peekable.peek(), Some(&1));
        assert_eq!(peekable.peek(), Some(&2));
        assert_eq!(peekable.peek(), Some(&3));

        // Test unpeek
        peekable.unpeek();
        assert_eq!(peekable.peek(), Some(&3));

        // Test iteration after peek/unpeek
        assert_eq!(peekable.next(), Some(1));
        assert_eq!(peekable.next(), Some(2));

        // Test unpeek on empty buffer
        assert_eq!(peekable.peek(), Some(&3));

        peekable.unpeek();
        assert_eq!(peekable.peek(), Some(&3));
    }

    #[test]
    fn test_with_next_pairs() {
        let pairs: Vec<_> = WithNext::new(vec![1, 2, 3].into_iter()).collect();
        assert_eq!(pairs, vec![(1, Some(2)), (2, Some(3)), (3, None)]);
    }

    #[test]
    fn test_with_next_single_element() {
        let pairs: Vec<_> = WithNext::new(vec![42].into_iter()).collect();
        assert_eq!(pairs, vec![(42, None)]);
    }

    #[test]
    fn test_with_next_empty() {
        let pairs: Vec<_> = WithNext::new(Vec::<i32>::new().into_iter()).collect();
        assert_eq!(pairs, vec![]);
    }
}
