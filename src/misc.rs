use chrono::{Duration, Months, NaiveDate, NaiveDateTime, Utc};
use std::sync::OnceLock;

/// Converts a `NaiveDate` to a `NaiveDateTime` at midnight (00:00:00).
pub fn to_datetime(date: NaiveDate) -> NaiveDateTime {
    date.and_hms_opt(0, 0, 0).unwrap()
}

static TODAY: OnceLock<NaiveDate> = OnceLock::new();
pub fn today() -> NaiveDate {
    *TODAY.get_or_init(|| Utc::now().date_naive())
}

/// A date range checker.
#[derive(Debug)]
pub enum BetweenDate {
    FromTo(NaiveDate, NaiveDate),
    From(NaiveDate),
    To(NaiveDate),
    Always,
}

impl BetweenDate {
    /// Creates a `BetweenDate` from optional `from` and `to` dates.
    ///
    /// # Arguments
    ///
    /// * `from` - Optional start date
    /// * `to` - Optional end date
    ///
    /// # Examples
    ///
    /// ```
    /// use chrono::NaiveDate;
    /// use ledger::misc::BetweenDate;
    ///
    /// let from = Some(NaiveDate::from_ymd_opt(2025,1,1).unwrap());
    /// let to   = Some(NaiveDate::from_ymd_opt(2025,12,31).unwrap());
    /// let between = BetweenDate::new(from, to);
    ///
    /// let date = NaiveDate::from_ymd_opt(2025,6,15).unwrap();
    /// assert!(between.check(date));
    /// ```
    pub fn new(from: Option<NaiveDate>, to: Option<NaiveDate>) -> Self {
        match (from, to) {
            (Some(f), Some(t)) => BetweenDate::FromTo(f, t),
            (Some(f), None) => BetweenDate::From(f),
            (None, Some(t)) => BetweenDate::To(t),
            (None, None) => BetweenDate::Always,
        }
    }

    /// Returns true if `d` is within the range.
    pub fn check(&self, d: NaiveDate) -> bool {
        match self {
            BetweenDate::FromTo(from, to) => d >= *from && d <= *to,
            BetweenDate::From(from) => d >= *from,
            BetweenDate::To(to) => d <= *to,
            BetweenDate::Always => true,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Step {
    Days(i32),
    Weeks(i32),
    Months(i32),
}

/// Iterates from `start`, advancing by days, weeks, or months.
/// - Always includes the initial date
/// - The sign indicates the direction
pub fn iter_dates(start: NaiveDate, step: Step) -> impl Iterator<Item = NaiveDate> {
    let mut curr = start;
    let mut remaining = match step {
        Step::Days(n) | Step::Weeks(n) | Step::Months(n) => n,
    };
    let mut finished = false;

    std::iter::from_fn(move || {
        if remaining == 0 {
            if finished {
                return None;
            }
            finished = true;
            return Some(curr);
        }

        let s = remaining.signum();
        remaining -= s;

        let res = curr;

        curr = match step {
            Step::Days(_) => curr + Duration::days(s as i64),
            Step::Weeks(_) => curr + Duration::days(7 * s as i64),
            Step::Months(_) => {
                if s > 0 {
                    curr.checked_add_months(Months::new(1)).unwrap()
                } else {
                    curr.checked_sub_months(Months::new(1)).unwrap()
                }
            }
        };

        Some(res)
    })
}
