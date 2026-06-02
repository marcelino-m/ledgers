use crate::journal::{self, parser};

/// Accept either a single transaction object or a list of them, shared
/// by the json and lisp decoders (the closures supply the
/// format-specific deserializer).
pub fn one_or_many<S, M>(single: S, many: M) -> Result<Vec<journal::Xact>, parser::ParseError>
where
    S: FnOnce() -> Result<parser::Xact, String>,
    M: FnOnce() -> Result<Vec<parser::Xact>, String>,
{
    match single() {
        Ok(x) => {
            let x = x.into_xact(0)?;
            Ok(vec![x])
        }
        Err(single_err) => match many() {
            Ok(xs) => {
                let mut out = Vec::new();
                for x in xs {
                    let x = x.into_xact(0)?;
                    out.push(x);
                }
                Ok(out)
            }
            Err(_) => Err(parser::ParseError::Deser(single_err)),
        },
    }
}

/// Decode `addx` input in JSON.
pub fn decode(content: &str) -> Result<Vec<journal::Xact>, parser::ParseError> {
    one_or_many(
        || serde_json::from_str::<parser::Xact>(content).map_err(|e| e.to_string()),
        || serde_json::from_str::<Vec<parser::Xact>>(content).map_err(|e| e.to_string()),
    )
}

#[cfg(test)]
mod test {
    use crate::journal::{parse_xacts_json, parse_xacts_ledger};
    use crate::printing::{self, Fmt};

    const SAMPLE: &str = "\
2012-01-01 * Opening  ; opening :Init:
    Assets:A   10 AAPL @ $5.00
    Equity:Open

2012-02-01 * Coffee  ; :Init:
    ;; memo: latte
    Expenses:Food   $4.50
    Assets:Cash
";

    /// Render transactions to the given output format, mirroring what
    /// `print --fmt FMT` emits.
    fn render(xacts: &[crate::journal::Xact], fmt: Fmt) -> String {
        let mut buf = Vec::new();
        printing::print::print(&mut buf, xacts.iter(), fmt).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn json_roundtrip_matches_tty() {
        let tty = parse_xacts_ledger(SAMPLE).unwrap();
        let json = render(&tty, Fmt::Json);
        let from_json = parse_xacts_json(&json).unwrap();
        assert_eq!(tty, from_json);
    }

    #[test]
    fn single_object_and_list_agree() {
        let obj = r#"{"date":"2012-03-01","state":"cleared","payee":"X",
            "postings":[{"account":"A","quantity":{"$":"1"}},{"account":"B"}]}"#;
        let list = format!("[{obj}]");
        assert_eq!(
            parse_xacts_json(obj).unwrap(),
            parse_xacts_json(&list).unwrap()
        );
    }

    #[test]
    fn elided_amount_inferred_like_tty() {
        let obj = r#"{"date":"2012-03-01","payee":"X",
            "postings":[{"account":"A","quantity":{"$":"1"}},{"account":"B"}]}"#;
        // No `*`: the json object has no `state`, i.e. State::None.
        let tty = parse_xacts_ledger("2012-03-01 X\n    A  $1\n    B\n").unwrap();
        assert_eq!(parse_xacts_json(obj).unwrap(), tty);
    }

    #[test]
    fn unbalanced_is_rejected() {
        let obj = r#"{"date":"2012-03-01","payee":"X",
            "postings":[{"account":"A","quantity":{"$":"1"}},{"account":"B","quantity":{"$":"2"}}]}"#;
        assert!(parse_xacts_json(obj).is_err());
    }
}
