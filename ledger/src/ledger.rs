use std::collections::HashMap;

use chrono::NaiveDate;

use crate::account::AccPostingSrc;
use crate::journal::{AccName, Journal, Posting, Xact};
use crate::misc::BetweenDate;

#[derive(Debug)]
pub struct Ledger<'l> {
    acc_posting: HashMap<&'l AccName, Vec<&'l Posting>>,
}

struct AccPosting<'a> {
    acc_name: AccName,
    postings: &'a Vec<&'a Posting>,
}

impl<'a> AccPostingSrc<'a> for AccPosting<'a> {
    fn acc_name(&self) -> &AccName {
        &self.acc_name
    }

    fn postings(&self) -> Box<dyn Iterator<Item = &'a Posting> + 'a> {
        Box::new(self.postings.iter().copied())
    }
}

impl<'l> Ledger<'l> {
    /// Creates a new [`Ledger`] from a list of transactions [`Xact`].
    pub fn from_journal(journal: &'l Journal) -> Ledger<'l> {
        let mut ledger = Ledger {
            acc_posting: HashMap::new(),
        };

        ledger.fill_from_xacts(journal.xacts());
        ledger
    }

    /// Returns a new [`Ledger`] containing only the accounts with transactions
    /// whose dates fall within the optional `from` and `to` date range.
    pub fn filter_by_date(&self, from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        let between = BetweenDate::new(from, to);

        let acc = self
            .acc_posting
            .iter()
            .map(|(&name, postings)| {
                (
                    name,
                    postings
                        .iter()
                        .filter(|&e| between.check(e.date))
                        .copied()
                        .collect(),
                )
            })
            .collect();

        Ledger { acc_posting: acc }
    }

    /// Returns an immutable reference to a ledger entry of an account
    /// by name.
    pub fn get_acc_postings<'a>(&'a self, name: &AccName) -> Option<impl AccPostingSrc<'a>> {
        self.acc_posting.get(name).map(|ps| AccPosting {
            acc_name: name.clone(),
            postings: ps,
        })
    }

    /// Returns an iterator over all accounts in the ledger.
    pub fn get_all_posting<'a>(&'a self) -> impl Iterator<Item = impl AccPostingSrc<'a>> {
        self.acc_posting.iter().map(|(&acc_name, ps)| AccPosting {
            acc_name: acc_name.clone(),
            postings: ps,
        })
    }

    /// Returns a mutable reference to a ledger entry of an account
    /// by name.
    fn get_entry_mut(&mut self, name: &'l AccName) -> &mut Vec<&'l Posting> {
        self.acc_posting.entry(name).or_default()
    }

    /// Populates the ledger by iterating over all transactions and
    /// postings and each posting is registered in the corresponding
    /// account.
    fn fill_from_xacts(&mut self, xacts: impl Iterator<Item = &'l Xact>) -> &mut Self {
        for xact in xacts {
            for p in &xact.postings {
                let acc = self.get_entry_mut(&p.acc_name);
                // TODO: optimize this: try to determine needed space
                acc.push(p);
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccPostingSrc;
    use crate::util;
    use chrono::NaiveDate;
    use std::io::Cursor;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn make_journal(input: &str) -> crate::journal::Journal {
        let bytes = input.to_owned().into_bytes();
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(Cursor::new(bytes)), None).unwrap();
        journal
    }

    #[test]
    fn from_journal_populates_accounts() {
        let input = "\
2026-01-01 salary
  Income:Salary   $100
  Assets:Cash
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let income = AccName::from("Income:Salary");
        let cash = AccName::from("Assets:Cash");

        assert!(ledger.get_acc_postings(&income).is_some());
        assert!(ledger.get_acc_postings(&cash).is_some());
    }

    #[test]
    fn acc_posting_acc_name_returns_correct_name() {
        let input = "\
2026-01-01 test
  Expenses:Food    $50
  Assets:Cash     $-50
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let food = AccName::from("Expenses:Food");
        let ps = ledger.get_acc_postings(&food).unwrap();
        assert_eq!(ps.acc_name(), &food);
    }

    #[test]
    fn get_acc_postings_returns_none_for_unknown_account() {
        let input = "\
2026-01-01 test
  A    $10
  B   $-10
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let unknown = AccName::from("C:Unknown");
        assert!(ledger.get_acc_postings(&unknown).is_none());
    }

    #[test]
    fn get_all_posting_returns_all_accounts() {
        let input = "\
2026-01-01 test
  A    $10
  B    $20
  C   $-30
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let mut names: Vec<String> = ledger
            .get_all_posting()
            .map(|ps| ps.acc_name().to_string())
            .collect();
        names.sort();

        assert_eq!(names, vec!["A", "B", "C"]);
    }

    #[test]
    fn filter_by_date_keeps_only_postings_in_range() {
        let input = "\
2026-01-01 early
  A    $10
  B   $-10

2026-06-01 mid
  A    $20
  B   $-20

2026-12-01 late
  A    $30
  B   $-30
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let filtered = ledger.filter_by_date(Some(d(2026, 3, 1)), Some(d(2026, 9, 1)));

        let acc_a = AccName::from("A");
        let ps = filtered.get_acc_postings(&acc_a).unwrap();
        let postings: Vec<_> = ps.postings().collect();
        // Only the mid posting (2026-06-01) should be kept
        assert_eq!(postings.len(), 1);
        assert_eq!(postings[0].date, d(2026, 6, 1));
    }

    #[test]
    fn filter_by_date_no_bounds_keeps_all() {
        let input = "\
2026-01-01 first
  A    $10
  B   $-10

2026-12-31 last
  A    $20
  B   $-20
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let filtered = ledger.filter_by_date(None, None);
        let acc_a = AccName::from("A");
        let ps = filtered.get_acc_postings(&acc_a).unwrap();
        let postings: Vec<_> = ps.postings().collect();
        assert_eq!(postings.len(), 2);
    }

    #[test]
    fn fill_from_xacts_multiple_transactions_same_account() {
        let input = "\
2026-01-01 first
  A    $10
  B   $-10

2026-02-01 second
  A    $20
  B   $-20
";
        let journal = make_journal(input);
        let ledger = Ledger::from_journal(&journal);

        let acc_a = AccName::from("A");
        let ps = ledger.get_acc_postings(&acc_a).unwrap();
        let postings: Vec<_> = ps.postings().collect();
        // Both postings to account A should be present
        assert_eq!(postings.len(), 2);
    }
}
