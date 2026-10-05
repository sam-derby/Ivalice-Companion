//! The game calendar: month 1-12 and day of a 365-day year, as the saved
//! Birthday and the calendar event variables (0x2E month, 0x2F day) store them.

const DAYS: [u16; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
/// Event variables holding the current month and day.
pub const MONTH_VARIABLE: u16 = 0x2e;
pub const DAY_VARIABLE: u16 = 0x2f;

/// One-based day in a 365-day year, or `None` for an invalid date.
#[must_use]
pub fn day_of_year(month: u8, day: u8) -> Option<u16> {
    let index = usize::from(month).checked_sub(1)?;
    let length = *DAYS.get(index)?;
    (1..=length)
        .contains(&u16::from(day))
        .then(|| DAYS[..index].iter().sum::<u16>() + u16::from(day))
}

/// Days in each month, January first.
#[must_use]
pub fn month_lengths() -> [u8; 12] {
    DAYS.map(|days| u8::try_from(days).unwrap_or(u8::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_follow_a_365_day_year() {
        assert_eq!(day_of_year(1, 1), Some(1));
        assert_eq!(day_of_year(3, 21), Some(80));
        assert_eq!(day_of_year(12, 31), Some(365));
        assert_eq!(
            month_lengths()
                .iter()
                .map(|days| u16::from(*days))
                .sum::<u16>(),
            365
        );
        for (month, day) in [(2, 29), (4, 31), (0, 1), (13, 1), (1, 0)] {
            assert_eq!(day_of_year(month, day), None, "{month}/{day}");
        }
    }
}
