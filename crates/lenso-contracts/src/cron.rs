use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use std::fmt;

const MAX_LOOKAHEAD_MINUTES: usize = 5 * 366 * 24 * 60;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSchedule {
    minutes: CronField,
    hours: CronField,
    days_of_month: CronField,
    months: CronField,
    days_of_week: CronField,
}

impl CronSchedule {
    pub fn parse(expression: &str) -> Result<Self, CronParseError> {
        let fields = expression.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 5 {
            return Err(CronParseError::new("cron expression must have 5 fields"));
        }

        Ok(Self {
            minutes: CronField::parse(fields[0], FieldKind::Minute)?,
            hours: CronField::parse(fields[1], FieldKind::Hour)?,
            days_of_month: CronField::parse(fields[2], FieldKind::DayOfMonth)?,
            months: CronField::parse(fields[3], FieldKind::Month)?,
            days_of_week: CronField::parse(fields[4], FieldKind::DayOfWeek)?,
        })
    }

    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut candidate = (after + Duration::minutes(1))
            .with_second(0)?
            .with_nanosecond(0)?;

        // ponytail: minute scan, replace with field-jumping if dense schedules become hot.
        for _ in 0..MAX_LOOKAHEAD_MINUTES {
            if self.matches(candidate) {
                return Some(candidate);
            }
            candidate += Duration::minutes(1);
        }

        None
    }

    fn matches(&self, at: DateTime<Utc>) -> bool {
        self.minutes.contains(at.minute())
            && self.hours.contains(at.hour())
            && self.months.contains(at.month())
            && self.matches_day(at)
    }

    fn matches_day(&self, at: DateTime<Utc>) -> bool {
        let dom_matches = self.days_of_month.contains(at.day());
        let dow_matches = self
            .days_of_week
            .contains(at.weekday().num_days_from_sunday());

        match (
            self.days_of_month.is_wildcard(),
            self.days_of_week.is_wildcard(),
        ) {
            (true, true) => true,
            (true, false) => dow_matches,
            (false, true) => dom_matches,
            (false, false) => dom_matches || dow_matches,
        }
    }
}

pub fn validate_cron_expression(expression: &str) -> Result<(), CronParseError> {
    CronSchedule::parse(expression).map(|_| ())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronParseError {
    message: String,
}

impl CronParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for CronParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CronParseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CronField {
    allowed: Vec<bool>,
    min: u32,
    max: u32,
    wildcard: bool,
}

impl CronField {
    fn parse(input: &str, kind: FieldKind) -> Result<Self, CronParseError> {
        let min = kind.min();
        let max = kind.max();
        let mut allowed = vec![false; (max + 1) as usize];
        let wildcard = input == "*";

        if input.trim().is_empty() {
            return Err(CronParseError::new("cron field must not be empty"));
        }

        for part in input.split(',') {
            parse_part(part, kind, &mut allowed)?;
        }

        Ok(Self {
            allowed,
            min,
            max,
            wildcard,
        })
    }

    fn contains(&self, value: u32) -> bool {
        if value < self.min || value > self.max {
            return false;
        }
        self.allowed[value as usize]
    }

    fn is_wildcard(&self) -> bool {
        self.wildcard
    }
}

fn parse_part(part: &str, kind: FieldKind, allowed: &mut [bool]) -> Result<(), CronParseError> {
    let (base, step) = match part.split_once('/') {
        Some((base, step)) => {
            let step = step
                .parse::<u32>()
                .map_err(|_| CronParseError::new("cron step must be a positive integer"))?;
            if step == 0 {
                return Err(CronParseError::new("cron step must be greater than zero"));
            }
            (base, step)
        }
        None => (part, 1),
    };

    let (start, end) = parse_range(base, kind, step != 1)?;
    let mut value = start;
    while value <= end {
        allowed[value as usize] = true;
        value = value.saturating_add(step);
        if value == 0 {
            break;
        }
    }

    Ok(())
}

fn parse_range(base: &str, kind: FieldKind, stepped: bool) -> Result<(u32, u32), CronParseError> {
    if base == "*" {
        return Ok((kind.min(), kind.max()));
    }

    if let Some((start, end)) = base.split_once('-') {
        let start = parse_value(start, kind)?;
        let end = parse_value(end, kind)?;
        if start > end {
            return Err(CronParseError::new("cron range start must be before end"));
        }
        return Ok((start, end));
    }

    let start = parse_value(base, kind)?;
    if stepped {
        Ok((start, kind.max()))
    } else {
        Ok((start, start))
    }
}

fn parse_value(value: &str, kind: FieldKind) -> Result<u32, CronParseError> {
    let normalized = value.to_ascii_uppercase();
    if let Some(named) = kind.named_value(&normalized) {
        return Ok(named);
    }

    let parsed = value
        .parse::<u32>()
        .map_err(|_| CronParseError::new("cron field contains an invalid value"))?;
    let parsed = if kind == FieldKind::DayOfWeek && parsed == 7 {
        0
    } else {
        parsed
    };
    if parsed < kind.min() || parsed > kind.max() {
        return Err(CronParseError::new("cron field value is out of range"));
    }
    Ok(parsed)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

impl FieldKind {
    fn min(self) -> u32 {
        match self {
            Self::Minute | Self::Hour | Self::DayOfWeek => 0,
            Self::DayOfMonth | Self::Month => 1,
        }
    }

    fn max(self) -> u32 {
        match self {
            Self::Minute => 59,
            Self::Hour => 23,
            Self::DayOfMonth => 31,
            Self::Month => 12,
            Self::DayOfWeek => 6,
        }
    }

    fn named_value(self, value: &str) -> Option<u32> {
        match self {
            Self::Month => month_value(value),
            Self::DayOfWeek => day_of_week_value(value),
            _ => None,
        }
    }
}

fn month_value(value: &str) -> Option<u32> {
    [
        ("JAN", 1),
        ("FEB", 2),
        ("MAR", 3),
        ("APR", 4),
        ("MAY", 5),
        ("JUN", 6),
        ("JUL", 7),
        ("AUG", 8),
        ("SEP", 9),
        ("OCT", 10),
        ("NOV", 11),
        ("DEC", 12),
    ]
    .into_iter()
    .find_map(|(name, parsed)| (name == value).then_some(parsed))
}

fn day_of_week_value(value: &str) -> Option<u32> {
    [
        ("SUN", 0),
        ("MON", 1),
        ("TUE", 2),
        ("WED", 3),
        ("THU", 4),
        ("FRI", 5),
        ("SAT", 6),
    ]
    .into_iter()
    .find_map(|(name, parsed)| (name == value).then_some(parsed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn cron_next_after_handles_steps_and_names() {
        let schedule = CronSchedule::parse("*/15 9-17 * JAN,MAR MON-FRI").expect("cron");
        let after = Utc.with_ymd_and_hms(2026, 1, 5, 9, 7, 10).unwrap();

        assert_eq!(
            schedule.next_after(after),
            Some(Utc.with_ymd_and_hms(2026, 1, 5, 9, 15, 0).unwrap())
        );
    }

    #[test]
    fn cron_day_of_month_or_day_of_week_matches_vixie_shape() {
        let schedule = CronSchedule::parse("0 9 15 * MON").expect("cron");

        assert_eq!(
            schedule.next_after(Utc.with_ymd_and_hms(2026, 6, 14, 9, 0, 0).unwrap()),
            Some(Utc.with_ymd_and_hms(2026, 6, 15, 9, 0, 0).unwrap())
        );
    }

    #[test]
    fn cron_rejects_invalid_shapes() {
        assert!(CronSchedule::parse("* * *").is_err());
        assert!(CronSchedule::parse("*/0 * * * *").is_err());
        assert!(CronSchedule::parse("60 * * * *").is_err());
    }
}
