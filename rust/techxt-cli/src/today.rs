//! Today's date, in the English form `\today` produces (PLAN.md §13).
//!
//! `Options::today` is a plain string, and the only thing standing between the system
//! clock and that string is a civil-calendar conversion and a table of month names. A
//! date/time crate would bring time zones, locales, parsing and formatting — none of
//! which this needs — so the calendar arithmetic is written out here instead.
//!
//! The split is deliberate: [`format_date`] and [`civil_from_days`] are pure functions
//! of their arguments and are unit-tested against fixed inputs, while [`today`] is the
//! one place that asks what time it is. A clock is not testable; a calendar is.

use std::time::{SystemTime, UNIX_EPOCH};

/// The month names `\today` spells out, indexed by month number minus one.
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// Today's date as `"August 19, 2026"`, read from the system clock in **UTC**.
///
/// UTC rather than local time because the alternative is a time-zone database: the
/// document says "today", and a rendering that is at most a few hours off at the day
/// boundary is a far better trade than a dependency on the world's daylight-saving
/// rules.
///
/// A clock reading before 1970 (the epoch) is the one case the arithmetic below still
/// handles honestly — [`civil_from_days`] works for negative day counts too — so a
/// misconfigured machine gets a wrong date, never a panic.
pub fn today() -> String {
    // `Duration` is unsigned, so the two directions from the epoch arrive as `Ok` and
    // `Err`; signing the second is what makes one division serve both. A reading too
    // large for `i64` saturates rather than wrapping — no clock produces one, and a
    // wrong date beats a panic if some machine does.
    let seconds = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(since) => i64::try_from(since.as_secs()).unwrap_or(i64::MAX),
        Err(before) => i64::try_from(before.duration().as_secs())
            .map(|seconds| -seconds)
            .unwrap_or(i64::MIN),
    };
    // Floor division, not truncation: a day *number* runs from midnight to midnight, so
    // every second of 1969-12-31 belongs to day -1.
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format_date(year, month, day)
}

/// Format a proleptic-Gregorian date the way LaTeX's `\today` does in English:
/// month name, day of month without a leading zero, comma, year.
///
/// `month` is 1-based. A month outside 1..=12 cannot arise from
/// [`civil_from_days`], so it is rendered as its own number rather than being made a
/// panic — the CLI must not abort over a date.
pub fn format_date(year: i32, month: u32, day: u32) -> String {
    match MONTH_NAMES.get((month.wrapping_sub(1)) as usize) {
        Some(name) => format!("{name} {day}, {year}"),
        None => format!("{month} {day}, {year}"),
    }
}

/// Convert a count of days since 1970-01-01 into a proleptic-Gregorian `(year, month,
/// day)`.
///
/// This is Howard Hinnant's `civil_from_days` (the algorithm C++20's `std::chrono`
/// adopted), which works by shifting the epoch to 0000-03-01 so that the leap day lands
/// at the *end* of the year and the 400-year cycle becomes plain arithmetic. It is exact
/// for every day in the range `i64` can express here, including days before the epoch.
///
/// The magic numbers, in order: 719468 is the day count from 0000-03-01 to 1970-01-01;
/// 146097 is the days in a 400-year era; 1461 is a 4-year cycle; 36524 a century; and
/// `(153 * m + 2) / 5` is the closed form of the March-based month-length sequence
/// 31, 30, 31, 30, 31, 31, 30, 31, 30, 31, 31, 28.
pub fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    // Era = the 400-year cycle the day falls in, floored so negative days work.
    let era = z.div_euclid(146_097);
    // Day of era, 0..=146096.
    let doe = z.rem_euclid(146_097);
    // Year of era, 0..=399.
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    // Year, still counted from March.
    let y = yoe + era * 400;
    // Day of the March-based year, 0..=365.
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    // March-based month index, 0..=11 (0 = March).
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    // Back to a January-based month, carrying the year over at the turn.
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_the_first_of_january_1970() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(format_date(1970, 1, 1), "January 1, 1970");
    }

    #[test]
    fn the_plans_own_example_round_trips() {
        // PLAN.md §13 spells the expected shape out: "August 19, 2026".
        // 2026-08-19 is 20684 days after the epoch.
        assert_eq!(civil_from_days(20_684), (2026, 8, 19));
        assert_eq!(format_date(2026, 8, 19), "August 19, 2026");
    }

    #[test]
    fn days_before_the_epoch_count_backwards() {
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        assert_eq!(civil_from_days(-365), (1969, 1, 1));
        assert_eq!(civil_from_days(-719_468), (0, 3, 1));
    }

    #[test]
    fn leap_days_land_where_the_gregorian_rules_put_them() {
        // 2000 is a leap year (divisible by 400), 1900 is not (divisible by 100).
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(-25_509), (1900, 2, 28));
        assert_eq!(civil_from_days(-25_508), (1900, 3, 1));
    }

    #[test]
    fn every_day_of_a_year_is_consecutive() {
        // A cheap total check on the arithmetic: walking a whole leap year day by day
        // must visit each month in order with the right number of days.
        let lengths = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
        let mut day_number = 19_723; // 2024-01-01
        for (index, length) in lengths.iter().enumerate() {
            for day in 1..=*length {
                assert_eq!(
                    civil_from_days(day_number),
                    (2024, index as u32 + 1, day),
                    "at day number {day_number}"
                );
                day_number += 1;
            }
        }
        assert_eq!(civil_from_days(day_number), (2025, 1, 1));
    }

    #[test]
    fn a_month_number_out_of_range_is_printed_rather_than_panicking() {
        assert_eq!(format_date(2026, 13, 1), "13 1, 2026");
        assert_eq!(format_date(2026, 0, 1), "0 1, 2026");
    }

    #[test]
    fn the_clock_reading_is_a_plausible_date() {
        // The only assertion a clock allows: the result is one of the twelve month names
        // followed by a day and a year in this century.
        let today = today();
        let (month, rest) = today.split_once(' ').expect("a month name and a date");
        assert!(MONTH_NAMES.contains(&month), "{today}");
        let (day, year) = rest.split_once(", ").expect("a day and a year");
        assert!(matches!(day.parse::<u32>(), Ok(1..=31)), "{today}");
        assert!(matches!(year.parse::<i32>(), Ok(2020..=2200)), "{today}");
    }
}
