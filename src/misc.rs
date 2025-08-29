use chrono::{NaiveDate, NaiveDateTime};

/// Converts a `NaiveDate` to a `NaiveDateTime` at midnight (00:00:00).
pub fn to_datetime(date: &NaiveDate) -> NaiveDateTime {
    date.and_hms_opt(0, 0, 0).unwrap()
}
