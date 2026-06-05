use crate::{
    Number,
    ast::{compact_decimal_number_text, compact_radix_number_text},
};

pub(crate) fn parse_bool(text: &str) -> Option<bool> {
    match text {
        "y" | "Y" | "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => {
            Some(true)
        }
        "n" | "N" | "no" | "No" | "NO" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" => {
            Some(false)
        }
        _ => None,
    }
}

pub(crate) fn is_null(text: &str) -> bool {
    text.is_empty() || text == "~" || text.eq_ignore_ascii_case("null")
}

pub(crate) fn parse_implicit_numeric_extension(text: &str) -> Option<Number> {
    if let Some(number) = parse_sexagesimal_number(text) {
        return Some(number);
    }
    let compact = compact_radix_number_text(text)?;
    parse_signed_integer_number(compact.as_ref(), false, true, false)
}

pub(crate) fn parse_explicit_int_number(text: &str) -> Option<Number> {
    if let Some(number) = parse_sexagesimal_number(text) {
        return Some(number);
    }
    let compact = compact_radix_number_text(text)?;
    parse_signed_integer_number(compact.as_ref(), true, true, true)
}

pub(crate) fn parse_explicit_float_legacy_number(text: &str) -> Option<Number> {
    if let Some(number) = parse_sexagesimal_number(text) {
        return number.as_f64().map(Number::from);
    }
    let compact = compact_radix_number_text(text)?;
    parse_signed_integer_number(compact.as_ref(), false, true, false)
        .and_then(|number| number.as_f64().map(Number::from))
}

pub(crate) fn parse_explicit_float_number(text: &str) -> Option<Number> {
    if let Some(compact) = compact_decimal_number_text(text) {
        let compact = compact.as_ref();
        if compact.eq_ignore_ascii_case(".nan") {
            return Some(Number::from(f64::NAN));
        }
        if compact.eq_ignore_ascii_case(".inf") || compact.eq_ignore_ascii_case("+.inf") {
            return Some(Number::from(f64::INFINITY));
        }
        if compact.eq_ignore_ascii_case("-.inf") {
            return Some(Number::from(f64::NEG_INFINITY));
        }
        if let Ok(value) = compact.parse::<f64>() {
            return Some(Number::from(value));
        }
    }
    if let Some(number) = parse_explicit_float_legacy_number(text) {
        return Some(number);
    }
    None
}

fn parse_signed_integer_number(
    compact: &str,
    allow_0o_octal: bool,
    allow_leading_zero_octal: bool,
    positive_i128: bool,
) -> Option<Number> {
    let (negative, rest) = match compact {
        text if text.starts_with('-') => (true, &text[1..]),
        text if text.starts_with('+') => (false, &text[1..]),
        text => (false, text),
    };
    let (radix, digits) =
        if let Some(digits) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
            (16, digits)
        } else if allow_0o_octal
            && let Some(digits) = rest.strip_prefix("0o").or_else(|| rest.strip_prefix("0O"))
        {
            (8, digits)
        } else if let Some(digits) = rest.strip_prefix("0b").or_else(|| rest.strip_prefix("0B")) {
            (2, digits)
        } else if allow_leading_zero_octal
            && rest.len() > 1
            && rest.starts_with('0')
            && rest.chars().all(|ch| ('0'..='7').contains(&ch))
        {
            (8, rest)
        } else if allow_0o_octal {
            (10, rest)
        } else {
            return None;
        };
    if digits.is_empty() {
        return None;
    }
    let magnitude = u128::from_str_radix(digits, radix).ok()?;
    signed_magnitude_number(negative, magnitude, positive_i128)
}

fn parse_sexagesimal_number(text: &str) -> Option<Number> {
    let groups = sexagesimal_groups(text)?;
    if groups.iter().any(|group| group.contains('.')) {
        return parse_sexagesimal_float(&groups).map(Number::Float);
    }
    parse_sexagesimal_integer(&groups)
}

fn sexagesimal_groups(text: &str) -> Option<Vec<&str>> {
    if !text.contains(':') || text.contains('_') {
        return None;
    }
    let groups = text.split(':').collect::<Vec<_>>();
    match groups.as_slice() {
        [first, second] if signed_digits(first) && unsigned_last_sexagesimal_group(second) => {}
        [first, second, _] if signed_digits(first) && unsigned_digits_below_60(second) => {}
        _ => return None,
    }
    if groups.len() == 3 && !unsigned_last_sexagesimal_group(groups[2]) {
        return None;
    }
    Some(groups)
}

fn parse_sexagesimal_integer(groups: &[&str]) -> Option<Number> {
    let (negative, first) = signed_first_group(groups[0])?;
    let mut magnitude = first.parse::<u128>().ok()?.checked_mul(3600)?;
    let minutes = u128::from(group_value_below_60(groups[1])?).checked_mul(60)?;
    magnitude = magnitude.checked_add(minutes)?;
    if let Some(seconds) = groups.get(2) {
        magnitude = magnitude.checked_add(u128::from(group_value_below_60(seconds)?))?;
    }
    signed_magnitude_number(negative, magnitude, false)
}

fn parse_sexagesimal_float(groups: &[&str]) -> Option<f64> {
    let (negative, first) = signed_first_group(groups[0])?;
    let first = first.parse::<f64>().ok()?;
    if groups.len() == 2 {
        let minutes = fractional_group_below_60(groups[1])?;
        let magnitude = first * 3600.0 + minutes * 60.0;
        let total = if negative { -magnitude } else { magnitude };
        return total.is_finite().then_some(total);
    }

    let mut magnitude = first * 3600.0;
    magnitude += f64::from(group_value_below_60(groups[1])?) * 60.0;
    magnitude += fractional_group_below_60(groups[2])?;
    let total = if negative { -magnitude } else { magnitude };
    total.is_finite().then_some(total)
}

/// Parses an unsigned base-60 digit group, requiring the value to be in `[0, 60)`.
fn group_value_below_60(text: &str) -> Option<u8> {
    if !text.is_empty() && text.chars().all(|ch| ch.is_ascii_digit()) {
        text.parse::<u8>().ok().filter(|&value| value < 60)
    } else {
        None
    }
}

/// Parses a possibly fractional base-60 group (e.g. `30.5`), requiring the
/// integer part to be in `[0, 60)` while allowing an arbitrary fraction.
fn fractional_group_below_60(text: &str) -> Option<f64> {
    let integer = text.split_once('.').map_or(text, |(integer, _)| integer);
    group_value_below_60(integer)?;
    text.parse::<f64>().ok()
}

fn signed_digits(text: &str) -> bool {
    let digits = text
        .strip_prefix('-')
        .or_else(|| text.strip_prefix('+'))
        .unwrap_or(text);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

fn unsigned_digits_below_60(text: &str) -> bool {
    group_value_below_60(text).is_some()
}

fn unsigned_last_sexagesimal_group(text: &str) -> bool {
    if let Some((integer, fraction)) = text.split_once('.') {
        return unsigned_digits_below_60(integer)
            && !fraction.is_empty()
            && fraction.chars().all(|ch| ch.is_ascii_digit());
    }
    unsigned_digits_below_60(text)
}

fn signed_first_group(text: &str) -> Option<(bool, &str)> {
    if let Some(rest) = text.strip_prefix('-') {
        Some((true, rest))
    } else if let Some(rest) = text.strip_prefix('+') {
        Some((false, rest))
    } else {
        Some((false, text))
    }
}

fn signed_magnitude_number(negative: bool, magnitude: u128, positive_i128: bool) -> Option<Number> {
    if negative {
        let min_magnitude = (i128::MAX as u128).saturating_add(1);
        if magnitude == min_magnitude {
            return Some(Number::Integer(i128::MIN));
        }
        let value = i128::try_from(magnitude).ok()?;
        return Some(Number::Integer(-value));
    }

    if positive_i128 && let Ok(value) = i128::try_from(magnitude) {
        Some(Number::Integer(value))
    } else if let Ok(value) = i64::try_from(magnitude) {
        Some(Number::Integer(i128::from(value)))
    } else {
        Some(Number::Unsigned(magnitude))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sexagesimal_seconds_within_range_is_accepted() {
        assert_eq!(
            parse_sexagesimal_number("1:30:59"),
            Some(Number::Integer(5459))
        );
    }

    #[test]
    fn sexagesimal_seconds_out_of_range_is_rejected() {
        // 60 is not a valid base-60 seconds field; fall back to a string scalar.
        assert_eq!(parse_sexagesimal_number("1:30:60"), None);
        // Likewise an obviously malformed seconds field.
        assert_eq!(parse_sexagesimal_number("1:30:999"), None);
    }

    #[test]
    fn sexagesimal_fractional_seconds_within_range_is_accepted() {
        assert_eq!(
            parse_sexagesimal_number("1:30:30.5"),
            Some(Number::Float(5430.5))
        );
    }

    #[test]
    fn sexagesimal_fractional_seconds_out_of_range_is_rejected() {
        assert_eq!(parse_sexagesimal_number("1:30:60.0"), None);
    }
}
