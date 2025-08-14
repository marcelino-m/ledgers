use crate::{commodity::Amount, journal::AccountName, ledger::Ledger};
use std::collections::HashMap;

/// The balance of a single account.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountBal {
    pub name: AccountName,
    pub balance: Amount,
}

/// Represents a financial balance as a collection of account
/// balances.
#[derive(Debug, PartialEq, Eq)]
pub struct Balance(Vec<AccountBal>);

/// Specifies the method to calculate an account balance or posting
/// value.
///
/// This enum determines whether the balance is computed using cost
/// basis, raw quantities, or the most recent known market value from
/// the price database.
///
/// # Variants
///
/// - `Basis`: Calculate using the historical cost (book value).
/// - `Quantity`: Calculate based on raw quantities without valuation.
pub enum Mode {
    Basis,
    Quantity,
}

/// Computes the trial balance for the given ledger.
///
/// Aggregates the balances of all accounts according to the specified mode.
///
/// # Parameters
///
/// - `ledger`: Reference to the [`Ledger`] to compute the trial balance from.
/// - `v`: [`Mode`] specifying whether to use the basis or quantity balance.
///
/// # Returns
///
/// A `Balance` containing the aggregated account balances according to the selected mode.
pub fn trial_balance<'a>(ledger: &'a Ledger, v: Mode) -> Balance {
    Balance(
        ledger
            .get_accounts()
            .map(|a| AccountBal {
                name: a.name.clone(),
                balance: match v {
                    Mode::Basis => a.book_balance(),
                    Mode::Quantity => a.balance(),
                },
            })
            .collect(),
    )
}

impl Balance {
    /// Returns a new `Balance` including the original accounts and all parent
    /// accounts with their cumulative sums.
    ///
    /// Consumes `self` and appends a new [`AccountBal`] for each parent path,
    /// where each parent's balance is the sum of all its child accounts.
    ///
    /// # Example
    ///
    /// ```
    /// let balance = Balance(vec![
    ///     AccountBal {
    ///         name: AccountName::from_str("Assets:Bank:Checking".to_string()),
    ///         balance: Amount::from(100),
    ///     },
    ///     AccountBal {
    ///         name: AccountName::from_str("Assets:Cash".to_string()),
    ///         balance: Amount::from(50),
    ///     },
    ///     AccountBal {
    ///         name: AccountName::from_str("Liabilities:Card".to_string()),
    ///         balance: Amount::from(25),
    ///     },
    /// ]);
    ///
    /// let cumulative = balance.balance_cumulative();
    ///
    /// // cumulative contains:
    /// // "Assets:Bank:Checking" => 100
    /// // "Assets:Cash"          => 50
    /// // "Assets"               => 150
    /// // "Liabilities:Card"     => 25
    /// ```
    pub fn balance_cumulative(mut self) -> Self {
        let mut cumsum = HashMap::new();
        for acc_bal in &self.0 {
            for p in acc_bal.name.parent_accounts() {
                let t = cumsum
                    .entry(AccountName::from_str(p.to_owned()))
                    .or_insert((0, Amount::default()));
                t.0 += 1;
                t.1 += &acc_bal.balance;
            }
        }

        let cumsum = cumsum
            .into_iter()
            .filter(|(_, (n, _))| *n > 1)
            .map(|(k, (_, v))| AccountBal {
                name: k,
                balance: v.clone(),
            });

        self.0.extend(cumsum);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity;
    use rust_decimal::dec;
    #[test]
    fn test_balance_cumulative() {
        let balance = Balance(vec![
            AccountBal {
                name: AccountName::from_str("Assets:Bank:Checking".to_string()),
                balance: quantity!(100, "$").to_amount(),
            },
            AccountBal {
                name: AccountName::from_str("Assets:Cash".to_string()),
                balance: quantity!(50, "$").to_amount(),
            },
            AccountBal {
                name: AccountName::from_str("Liabilities:Card".to_string()),
                balance: quantity!(25, "$").to_amount(),
            },
        ]);

        let mut balance = balance.balance_cumulative();

        let mut expected = Balance(vec![
            AccountBal {
                name: AccountName::from_str("Assets:Bank:Checking".to_string()),
                balance: quantity!(100, "$").to_amount(),
            },
            AccountBal {
                name: AccountName::from_str("Assets:Cash".to_string()),
                balance: quantity!(50, "$").to_amount(),
            },
            AccountBal {
                name: AccountName::from_str("Liabilities:Card".to_string()),
                balance: quantity!(25, "$").to_amount(),
            },
            AccountBal {
                name: AccountName::from_str("Assets".to_string()),
                balance: quantity!(150, "$").to_amount(), // 100 + 50
            },
        ]);

        balance.0.sort_by(|a, b| a.name.cmp(&b.name));
        expected.0.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(balance.0, expected.0)
    }
}
