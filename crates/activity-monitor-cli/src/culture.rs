//! Culture-aware string ordering matching the .NET original: LINQ's
//! `OrderBy(x => x)` on strings uses `Comparer<string>.Default`, i.e.
//! `string.CompareTo` under CurrentCulture — on .NET 5+ that is an ICU
//! collator at the user-default locale with default (tertiary) strength.
//! Ordinal `str::cmp` differs (e.g. "Zebra" < "apple" ordinally, but
//! "apple" < "Zebra" in en-US culture), which was the one cosmetic report
//! divergence left after the port.

use std::cmp::Ordering;
use std::sync::OnceLock;

use icu::collator::options::CollatorOptions;
use icu::collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu::locale::{Locale, locale};

fn collator() -> &'static CollatorBorrowed<'static> {
    static C: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
    C.get_or_init(|| {
        let name = user_locale();
        let loc: Locale = name.parse().unwrap_or_else(|_| locale!("en-US"));
        Collator::try_new(CollatorPreferences::from(&loc), CollatorOptions::default()).unwrap_or_else(
            |_| {
                Collator::try_new(
                    CollatorPreferences::from(&locale!("en-US")),
                    CollatorOptions::default(),
                )
                .expect("built-in en-US collator")
            },
        )
    })
}

/// Windows user-default locale (what .NET CurrentCulture uses), e.g. "en-US".
#[cfg(windows)]
fn user_locale() -> String {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;
    let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len > 1 {
        String::from_utf16_lossy(&buf[..(len as usize) - 1])
    } else {
        "en-US".to_string()
    }
}

#[cfg(not(windows))]
fn user_locale() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .ok()
        .and_then(|v| v.split('.').next().map(str::to_string))
        .filter(|v| !v.is_empty() && v != "C")
        .unwrap_or_else(|| "en-US".to_string())
}

pub fn culture_cmp(a: &str, b: &str) -> Ordering {
    collator().compare(a, b)
}

/// Stable culture-aware sort — matches LINQ OrderBy (stable, CurrentCulture).
pub fn culture_sort(v: &mut [String]) {
    v.sort_by(|a, b| culture_cmp(a, b));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn culture_not_ordinal() {
        // Ordinal would give ["Zebra", "apple"]; culture gives apple first.
        let mut v = vec!["Zebra".to_string(), "apple".to_string()];
        culture_sort(&mut v);
        assert_eq!(v, vec!["apple".to_string(), "Zebra".to_string()]);
    }

    #[test]
    fn culture_case_insensitive_primary() {
        let mut v = vec!["banana".to_string(), "Apple".to_string(), "cherry".to_string()];
        culture_sort(&mut v);
        assert_eq!(v, vec!["Apple".to_string(), "banana".to_string(), "cherry".to_string()]);
    }

    #[test]
    fn culture_sort_orders_ascending() {
        let mut v = vec!["b".to_string(), "a".to_string(), "c".to_string()];
        culture_sort(&mut v);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }
}
