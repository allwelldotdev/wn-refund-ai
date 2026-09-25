//! Money is integer cents everywhere (ADR-013); this is the only place it becomes text.

/// `129900` → `"$1,299.00"`, `-5` → `"-$0.05"`.
pub fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.unsigned_abs();
    let dollars = (abs / 100).to_string();
    let mut grouped = String::with_capacity(dollars.len() + dollars.len() / 3);
    for (i, digit) in dollars.chars().enumerate() {
        if i > 0 && (dollars.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{sign}${grouped}.{:02}", abs % 100)
}

#[cfg(test)]
mod tests {
    use super::format_cents;

    #[test]
    fn formats_dollars_and_cents_with_grouping() {
        assert_eq!(format_cents(0), "$0.00");
        assert_eq!(format_cents(5), "$0.05");
        assert_eq!(format_cents(8999), "$89.99");
        assert_eq!(format_cents(50000), "$500.00");
        assert_eq!(format_cents(129900), "$1,299.00");
        assert_eq!(format_cents(100_000_000), "$1,000,000.00");
        assert_eq!(format_cents(-5), "-$0.05");
        assert_eq!(format_cents(i64::MIN), "-$92,233,720,368,547,758.08");
    }
}
