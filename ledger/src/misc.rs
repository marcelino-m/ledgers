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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn check_from_to_inside() {
        let bd = BetweenDate::new(Some(d(2025, 1, 1)), Some(d(2025, 12, 31)));
        assert!(bd.check(d(2025, 6, 15)));
    }

    #[test]
    fn check_from_to_on_boundaries() {
        let bd = BetweenDate::new(Some(d(2025, 1, 1)), Some(d(2025, 12, 31)));
        assert!(bd.check(d(2025, 1, 1)));
        assert!(bd.check(d(2025, 12, 31)));
    }

    #[test]
    fn check_from_to_outside() {
        let bd = BetweenDate::new(Some(d(2025, 1, 1)), Some(d(2025, 12, 31)));
        assert!(!bd.check(d(2024, 12, 31)));
        assert!(!bd.check(d(2026, 1, 1)));
    }

    #[test]
    fn check_from_only_accepts_after() {
        let bd = BetweenDate::new(Some(d(2025, 6, 1)), None);
        assert!(bd.check(d(2025, 6, 1)));
        assert!(bd.check(d(2030, 1, 1)));
        assert!(!bd.check(d(2025, 5, 31)));
    }

    #[test]
    fn check_to_only_accepts_before() {
        let bd = BetweenDate::new(None, Some(d(2025, 6, 1)));
        assert!(bd.check(d(2025, 6, 1)));
        assert!(bd.check(d(2020, 1, 1)));
        assert!(!bd.check(d(2025, 6, 2)));
    }

    #[test]
    fn check_always_accepts_any_date() {
        let bd = BetweenDate::new(None, None);
        assert!(bd.check(d(2000, 1, 1)));
        assert!(bd.check(d(2099, 12, 31)));
    }

    #[test]
    fn iter_dates_days_positive() {
        // Step::Days(3) yields start + 3 steps = 4 dates
        let dates: Vec<_> = iter_dates(d(2024, 12, 31), Step::Days(4)).collect();
        assert_eq!(
            dates,
            vec![
                d(2024, 12, 31),
                d(2025, 1, 1),
                d(2025, 1, 2),
                d(2025, 1, 3),
                d(2025, 1, 4)
            ]
        );
    }

    #[test]
    fn iter_dates_days_negative() {
        let dates: Vec<_> = iter_dates(d(2025, 1, 4), Step::Days(-4)).collect();
        assert_eq!(
            dates,
            vec![
                d(2025, 1, 4),
                d(2025, 1, 3),
                d(2025, 1, 2),
                d(2025, 1, 1),
                d(2024, 12, 31)
            ]
        );
    }

    #[test]
    fn iter_dates_days_zero() {
        let dates: Vec<_> = iter_dates(d(2025, 6, 15), Step::Days(0)).collect();
        assert_eq!(dates, vec![d(2025, 6, 15)]);
    }

    #[test]
    fn iter_dates_weeks_positive() {
        let dates: Vec<_> = iter_dates(d(2024, 12, 18), Step::Weeks(2)).collect();
        assert_eq!(dates, vec![d(2024, 12, 18), d(2024, 12, 25), d(2025, 1, 1)]);
    }

    #[test]
    fn iter_dates_weeks_negative() {
        let dates: Vec<_> = iter_dates(d(2025, 1, 1), Step::Weeks(-2)).collect();
        assert_eq!(dates, vec![d(2025, 1, 1), d(2024, 12, 25), d(2024, 12, 18)]);
    }

    #[test]
    fn iter_dates_weeks_zero() {
        let dates: Vec<_> = iter_dates(d(2025, 3, 10), Step::Weeks(0)).collect();
        assert_eq!(dates, vec![d(2025, 3, 10)]);
    }

    #[test]
    fn iter_dates_months_positive() {
        let dates: Vec<_> = iter_dates(d(2024, 11, 15), Step::Months(3)).collect();
        assert_eq!(
            dates,
            vec![
                d(2024, 11, 15),
                d(2024, 12, 15),
                d(2025, 1, 15),
                d(2025, 2, 15)
            ]
        );
    }

    #[test]
    fn iter_dates_months_negative() {
        let dates: Vec<_> = iter_dates(d(2025, 2, 15), Step::Months(-3)).collect();
        assert_eq!(
            dates,
            vec![
                d(2025, 2, 15),
                d(2025, 1, 15),
                d(2024, 12, 15),
                d(2024, 11, 15)
            ]
        );
    }

    #[test]
    fn iter_dates_months_zero() {
        let dates: Vec<_> = iter_dates(d(2025, 7, 1), Step::Months(0)).collect();
        assert_eq!(dates, vec![d(2025, 7, 1)]);
    }
}
