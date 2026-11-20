//! Timestamp contract: TEXT "O" format UTC, byte-compatible with
//! DateTime.UtcNow.ToString("O") â€” exactly 7 fractional digits, Z suffix.

use jiff::tz::TimeZone;
use jiff::Timestamp;

pub fn format_o(ts: &Timestamp) -> String {
    // %7f prints exactly seven 100ns digits, never trimming zeros (unlike %f).
    ts.strftime("%Y-%m-%dT%H:%M:%S.%7fZ").to_string()
}

pub fn now_o() -> String {
    format_o(&Timestamp::now())
}

pub fn parse_o(s: &str) -> Result<Timestamp, jiff::Error> {
    s.parse()
}

/// Parse a "YYYY-MM-DD" (or full civil datetime) string as a local calendar date.
pub fn parse_local_date(s: &str) -> Result<jiff::civil::Date, jiff::Error> {
    match s.parse::<jiff::civil::Date>() {
        Ok(d) => Ok(d),
        Err(_) => Ok(s.parse::<jiff::civil::DateTime>()?.date()),
    }
}

/// Local midnight of the given calendar date, as a UTC "O" string.
pub fn date_midnight_utc_o(d: &jiff::civil::Date) -> String {
    let ts = d
        .at(0, 0, 0, 0)
        .to_zoned_datetime(TimeZone::system())
        .map(|z| z.timestamp())
        .unwrap_or_else(|_| Timestamp::now());
    format_o(&ts)
}

/// Local calendar date (or datetime) string, interpreted in the system time
/// zone and returned as a UTC "O" string. This is the fix for the C# tz bug:
/// DateTime.Parse + ToString("O") produced local-offset strings that compared
/// wrongly against the Z-suffixed UTC rows in BETWEEN clauses.
pub fn local_date_to_utc_o(s: &str) -> Result<String, jiff::Error> {
    Ok(date_midnight_utc_o(&parse_local_date(s)?))
}

/// Local midnight of the day AFTER the given local date, as a UTC "O" string
/// (replaces DateTime.Parse(toDate).AddDays(1).ToUniversalTime().ToString("O")).
pub fn next_day_local_midnight_utc_o(s: &str) -> Result<String, jiff::Error> {
    Ok(date_midnight_utc_o(&parse_local_date(s)?.tomorrow()?))
}

pub fn today_local_date() -> jiff::civil::Date {
    Timestamp::now().to_zoned_datetime(TimeZone::system()).date()
}

/// Local midnight today, as a UTC "O" string (replaces DateTime.Today.ToString("O")).
pub fn today_start_utc_o() -> String {
    date_midnight_utc_o(&today_local_date())
}

/// `days` ago from now, as a UTC "O" string (replaces DateTime.Now.AddDays(-n).ToString("O")).
pub fn days_ago_o(days: i64) -> String {
    let secs = days.max(0) as u64 * 86_400;
    format_o(&(Timestamp::now() - std::time::Duration::from_secs(secs)))
}

/// Stored UTC timestamp rendered in the local zone as "yyyy-MM-dd HH:mm:ss".
pub fn utc_to_local_display(ts: &Timestamp) -> String {
    ts.to_zoned_datetime(TimeZone::system()).strftime("%Y-%m-%d %H:%M:%S").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_digit_fraction_preserved() {
        let ts = parse_o("2026-11-13T12:34:56.7654321Z").unwrap();
        assert_eq!(format_o(&ts), "2026-11-13T12:34:56.7654321Z");
    }

    #[test]
    fn zero_fraction_keeps_seven_digits() {
        let ts = parse_o("2026-11-13T12:34:56Z").unwrap();
        assert_eq!(format_o(&ts), "2026-11-13T12:34:56.0000000Z");
    }

    #[test]
    fn roundtrip() {
        let s = now_o();
        assert!(s.ends_with('Z'));
        assert_eq!(s.len(), 28); // 19 + '.' + 7 + 'Z'
        parse_o(&s).unwrap();
    }

    #[test]
    fn local_date_converts_to_utc_o() {
        let s = local_date_to_utc_o("2026-11-13").unwrap();
        assert!(s.ends_with('Z'));
        assert_eq!(s.len(), 28);
        // same calendar day modulo the local offset (offset is < 24h)
        assert!(s.starts_with("2026-11-1"));
        // full datetime string also parses
        let s2 = local_date_to_utc_o("2026-11-13T10:30:00").unwrap();
        assert_eq!(s2.len(), 28);
    }

    #[test]
    fn today_start_and_days_ago_are_o_format() {
        assert_eq!(today_start_utc_o().len(), 28);
        assert_eq!(days_ago_o(7).len(), 28);
        assert!(days_ago_o(0).ends_with('Z'));
    }

    #[test]
    fn next_day_midnight_is_24h_after_date_midnight() {
        let from = local_date_to_utc_o("2026-11-13").unwrap();
        let next = next_day_local_midnight_utc_o("2026-11-13").unwrap();
        let a = parse_o(&from).unwrap();
        let b = parse_o(&next).unwrap();
        let diff = (b - a).as_secs();
        assert_eq!(diff, 86_400);
    }
}
