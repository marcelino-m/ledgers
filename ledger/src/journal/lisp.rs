use super::json;
use crate::journal::{self, parser};

/// Decode `addx` input in lisp (S-expression) format.
pub fn decode(content: &str) -> Result<Vec<journal::Xact>, parser::ParseError> {
    json::one_or_many(
        || serde_lexpr::from_str::<parser::Xact>(content).map_err(|e| e.to_string()),
        || serde_lexpr::from_str::<Vec<parser::Xact>>(content).map_err(|e| e.to_string()),
    )
}

#[cfg(test)]
mod test {
    use crate::journal::{parse_xacts_ledger, parse_xacts_lisp};
    use crate::printing::{self, Fmt};

    const SAMPLE: &str = "\
2012-01-01 * Opening  ; opening :Init:
    Assets:A   10 AAPL @ $5.00
    Equity:Open

2012-02-01 * Coffee  ; memo: latte
    Expenses:Food   $4.50
    Assets:Cash
";

    fn render_lisp(xacts: &[crate::journal::Xact]) -> String {
        let mut buf = Vec::new();
        printing::print::print(&mut buf, xacts.iter(), Fmt::Lisp).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn lisp_roundtrip_matches_tty() {
        let tty = parse_xacts_ledger(SAMPLE).unwrap();
        let lisp = render_lisp(&tty);
        let from_lisp = parse_xacts_lisp(&lisp).unwrap();
        assert_eq!(tty, from_lisp);
    }

    #[test]
    fn lisp_single_object_matches_list() {
        let tty = parse_xacts_ledger("2012-04-01 * Solo\n    A  $7\n    B\n").unwrap();
        let list = render_lisp(&tty); // a one-element list: ( (..) )
        // Drop the outer parentheses to get a single object.
        let trimmed = list.trim();
        let object = &trimmed[1..trimmed.len() - 1];
        assert_eq!(
            parse_xacts_lisp(object).unwrap(),
            parse_xacts_lisp(&list).unwrap()
        );
    }
}
