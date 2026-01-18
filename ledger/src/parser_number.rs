use regex::Regex;
use rust_decimal::Decimal;
use std::{collections::HashMap, sync::OnceLock};

pub fn parse(input: &str, f: NumberFormat) -> Option<Decimal> {
    let is = is_format(input, f);
    if !is {
        return None;
    }

    let cleaned_input = match f {
        NumberFormat::Us => input.replace(",", ""),
        NumberFormat::European => input.replace(".", "").replace(",", "."),
        NumberFormat::French => input.replace(" ", "").replace(",", "."),
        NumberFormat::Swiss => input.replace("'", ""),
        NumberFormat::Plain => input.to_string(),
        NumberFormat::Indian => input.replace(",", ""),
    };

    Decimal::from_str_exact(&cleaned_input).ok()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NumberFormat {
    Us,       // 1,234,567.89
    European, // 1.234.567,89
    French,   // 1 234 567,89
    Swiss,    // 1'234'567.89
    Indian,   // 12,34,567.89
    Plain,    // 1234567.89
}

fn is_format(input: &str, f: NumberFormat) -> bool {
    patterns().get(&f).unwrap().is_match(input)
}
static PATTERNS: OnceLock<HashMap<NumberFormat, Regex>> = OnceLock::new();

fn patterns() -> &'static HashMap<NumberFormat, Regex> {
    PATTERNS.get_or_init(|| {
        let mut m = HashMap::new();

        m.insert(
            NumberFormat::Us,
            Regex::new(r"^[+-]?\d{1,3}(,\d{3})*(\.\d+)?$").unwrap(),
        );

        m.insert(
            NumberFormat::European,
            Regex::new(r"^[+-]?\d{1,3}(\.\d{3})*(,\d+)?$").unwrap(),
        );

        m.insert(
            NumberFormat::Swiss,
            Regex::new(r"^[+-]?\d{1,3}('\d{3})*(\.\d+)?$").unwrap(),
        );

        m.insert(
            NumberFormat::French,
            Regex::new(r"^[+-]?\d{1,3}( \d{3})*(,\d+)?$").unwrap(),
        );

        m.insert(
            NumberFormat::Plain,
            Regex::new(r"^[+-]?\d+(\.\d+)?$").unwrap(),
        );

        m.insert(
            NumberFormat::Indian,
            Regex::new(r"^[+-]?(?:0|[1-9]\d{0,2})(?:,\d{2})*,\d{3}(?:\.\d+)?$").unwrap(),
        );

        m
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn parse_plain_numbers() {
        assert_eq!(parse("0", NumberFormat::Plain), Some(d("0")));
        assert_eq!(parse("10", NumberFormat::Plain), Some(d("10")));
        assert_eq!(parse("1234.56", NumberFormat::Plain), Some(d("1234.56")));
        assert_eq!(parse("-42.5", NumberFormat::Plain), Some(d("-42.5")));
    }

    #[test]
    fn parse_us_format() {
        assert_eq!(parse("1,234", NumberFormat::Us), Some(d("1234")));
        assert_eq!(parse("1,234.56", NumberFormat::Us), Some(d("1234.56")));
        assert_eq!(
            parse("-12,345,678.90", NumberFormat::Us),
            Some(d("-12345678.90"))
        );
        assert_eq!(parse("-90", NumberFormat::Us), Some(d("-90")));
    }

    #[test]
    fn parse_european_format() {
        assert_eq!(parse("1.234", NumberFormat::European), Some(d("1234")));
        assert_eq!(
            parse("1.234,56", NumberFormat::European),
            Some(d("1234.56"))
        );
        assert_eq!(
            parse("-12.345.678,90", NumberFormat::European),
            Some(d("-12345678.90"))
        );
        assert_eq!(parse("34", NumberFormat::European), Some(d("34")));
    }

    #[test]
    fn parse_french_format() {
        assert_eq!(parse("1 234", NumberFormat::French), Some(d("1234")));
        assert_eq!(parse("1 234,56", NumberFormat::French), Some(d("1234.56")));
        assert_eq!(
            parse("12 345 678,90", NumberFormat::French),
            Some(d("12345678.90"))
        );
        assert_eq!(
            parse("-12 345 678,90", NumberFormat::French),
            Some(d("-12345678.90"))
        );
    }

    #[test]
    fn parse_swiss_format() {
        assert_eq!(parse("1'234", NumberFormat::Swiss), Some(d("1234")));
        assert_eq!(parse("1'234.56", NumberFormat::Swiss), Some(d("1234.56")));
        assert_eq!(
            parse("12'345'678.90", NumberFormat::Swiss),
            Some(d("12345678.90"))
        );
    }

    #[test]
    fn parse_indian_format() {
        assert_eq!(parse("1,234", NumberFormat::Indian), Some(d("1234")));
        assert_eq!(
            parse("1,23,456.78", NumberFormat::Indian),
            Some(d("123456.78"))
        );
        assert_eq!(
            parse("12,34,56,789.00", NumberFormat::Indian),
            Some(d("123456789.00"))
        );
        assert_eq!(
            parse("-12,34,56,789.00", NumberFormat::Indian),
            Some(d("-123456789.00"))
        );
        assert_eq!(parse("-12,34.00", NumberFormat::Indian), None);
        assert_eq!(parse("-12,34,32.00", NumberFormat::Indian), None);
    }

    #[test]
    fn parse_with_leading_and_trailing_zeros() {
        assert_eq!(parse("001,234.500", NumberFormat::Us), Some(d("1234.500")));
        assert_eq!(parse("000", NumberFormat::Us), Some(d("0")));
    }
}
