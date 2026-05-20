use chrono::NaiveDate;
use regex::Regex;

use crate::{journal::Xact, misc::BetweenDate};

/// Returns an iterator over transactions whose date is within
/// `[from, to]` and that have at least one posting whose account
/// name matches one of `qry`.
///
/// An empty `qry` matches every transaction. When any posting of a
/// transaction matches, the entire transaction is yielded unchanged so
/// the output keeps the balanced-transaction invariant.
pub fn print<'a>(
    xacts: impl Iterator<Item = &'a Xact> + 'a,
    qry: &'a [Regex],
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
) -> impl Iterator<Item = &'a Xact> + 'a {
    let between = BetweenDate::new(from, to);
    xacts
        .filter(move |x| between.check(x.date.txdate))
        .filter(move |x| {
            qry.is_empty()
                || x.postings
                    .iter()
                    .any(|p| qry.iter().any(|r| r.is_match(&p.acc_name)))
        })
}
