use std::collections::BTreeSet;

use serde::Serialize;

use crate::{
    journal::{AccName, Xact},
    symbol::Symbol,
};

/// Catalog of what exists in a journal: accounts, commodities, and
/// similar metadata.
#[derive(Serialize)]
pub struct JnlInfo {
    pub accounts: Vec<AccName>,
    pub commodities: Vec<Symbol>,
    pub payees: Vec<String>,
}

/// Walks the given transactions and builds a [`JnlInfo`]
/// describing their contents.
pub fn scan<'a>(xacts: impl Iterator<Item = &'a Xact>) -> JnlInfo {
    let mut accounts = BTreeSet::new();
    let mut commodities = BTreeSet::new();
    let mut payees = BTreeSet::new();

    for xact in xacts {
        payees.insert(xact.payee.clone());
        for ps in &xact.postings {
            accounts.insert(ps.acc_name.clone());
            commodities.insert(ps.quantity.s);
        }
    }

    JnlInfo {
        accounts: accounts.into_iter().collect(),
        commodities: commodities.into_iter().collect(),
        payees: payees.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::util;

    fn make_report(input: &str) -> JnlInfo {
        let bytes = input.to_owned().into_bytes();
        let (journal, _) =
            util::read_journal_and_price_db(Box::new(Cursor::new(bytes)), None).unwrap();
        scan(journal.xacts())
    }

    #[test]
    fn collect_accounts_and_commodities() {
        let input = "\
2026-01-01 salary
  Assets:Bank     $1
  Income:Salary  $-1
";
        let report = make_report(input);
        assert!(report.accounts.contains(&AccName::from("Assets:Bank")));
        assert!(report.accounts.contains(&AccName::from("Income:Salary")));
        assert!(report.commodities.contains(&Symbol::new("$")));
    }

    #[test]
    fn accounts_and_commodities_sorted_and_deduped() {
        let input = "\
2026-01-01 test
  A   $1
  B  $-1

2026-01-02 test2
  A   $1
  B  $-1
";
        let report = make_report(input);
        assert_eq!(
            report.accounts,
            vec![AccName::from("A"), AccName::from("B")]
        );
        assert_eq!(report.commodities, vec![Symbol::new("$")]);
    }

    #[test]
    fn empty_journal_returns_empty_report() {
        let report = make_report("");
        assert!(report.accounts.is_empty());
        assert!(report.commodities.is_empty());
        assert!(report.payees.is_empty());
    }

    #[test]
    fn payees_sorted_and_deduped() {
        let input = "\
2026-01-01 grocery
  A   $1
  B  $-1

2026-01-02 salary
  A   $1
  B  $-1

2026-01-03 grocery
  A   $1
  B  $-1
";
        let report = make_report(input);
        assert_eq!(report.payees, vec!["grocery".to_string(), "salary".to_string()]);
    }

    #[test]
    fn empty_symbol_excluded_from_commodities() {
        let input = "\
2026-01-01 test
  A   $1
  B
";
        let report = make_report(input);
        assert!(!report.commodities.contains(&Symbol::new("")));
    }
}
