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
