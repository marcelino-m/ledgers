use crate::{account::AccountName, commodity::Amount, ledger::Ledger};

use regex::Regex;

use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};

/// The balance of a single account.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountBal {
    pub name: AccountName,
    pub balance: Amount,
    pub sub_account: Option<BTreeMap<AccountName, AccountBal>>,
}

/// Represents a financial balance as a collection of account
/// balances.
#[derive(Debug, PartialEq, Eq)]
pub struct Balance(BTreeMap<AccountName, AccountBal>);

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
pub fn trial_balance<'a>(ledger: &'a Ledger, v: Mode, qry: &[Regex]) -> Balance {
    Balance(
        ledger
            .get_accounts()
            .filter(|accnt| qry.is_empty() || qry.iter().any(|r| r.is_match(&accnt.name)))
            .map(|a| AccountBal {
                name: a.name.clone(),
                balance: match v {
                    Mode::Basis => a.book_balance(),
                    Mode::Quantity => a.balance(),
                },
                sub_account: None,
            })
            .map(|accn| (accn.name.clone(), accn))
            .collect(),
    )
}

impl Balance {
    /// Build a empty Balance
    pub fn new() -> Balance {
        Balance(BTreeMap::new())
    }

    /// Take a flatten Balance and transform to a hierarchical one
    ///
    ///```text`
    /// Flat View                     Hierarchical View
    /// ---------------               -------------------
    ///
    /// Assets:Bank (250)             Assets (200)
    /// Assets:Bank:Checking (200)    |-- Bank (250)
    /// Assets:Bank:Savings (50)      |   |-- Checking (200)
    ///                               |   `-- Savings (50)
    ///```
    pub fn to_hierarchical(self) -> Self {
        let mut balance = Balance::new();
        for accnt in self.0.values() {
            balance.add_account_bal(accnt);
        }

        balance.to_compact()
    }

    /// Simplifies an account hierarchy by collapsing mergeable
    /// sub-accounts.
    ///
    /// ## Example:
    /// Before `to_compact`:
    ///
    /// ```text
    /// Assets (200)               Liabilities (300)
    /// |-- Bank (200)             |-- CreditCard (300)
    /// |   `-- Checking (200)     |   `-- Visa (300)
    /// |-- Investments (200)      |-- Loan (300)
    ///     |-- Stocks (150)           `-- Mortgage (300)
    ///     `-- Bonds (50)
    ///```
    /// After `to_compact`:
    ///
    ///```text
    /// Assets:Bank:Checking (200)  Liabilities:CreditCard:Visa (300)
    /// Assets:Investments          Liabilities:Loan:Mortgage (300)
    /// |-- Stocks (150)
    /// `-- Bo ends (50)
    /// ```
    pub fn to_compact(self) -> Self {
        let mut bal = Balance::new();

        for (k, mut accnt) in self.0.into_iter() {
            let amount = accnt.balance.clone();
            let name = Balance::merge_subaccounts(&mut accnt, &AccountName::from(""), &amount);
            if let Some(name) = name {
                accnt.name = name;
                accnt.sub_account = None
            }
            bal.insert(k, accnt);
        }
        bal
    }
    /// Adds or updates an account balance, creating parent accounts as needed.
    /// # Example
    ///
    /// ```text
    /// Assets (100) + Assets:Bank:Checking (50) ->
    /// Assets (150)
    /// |-- Bank (50)
    ///     `-- Checking (50)
    /// ```
    pub fn add_account_bal(&mut self, account: &AccountBal) {
        let part: Vec<&str> = account.name.split_parts().collect();
        let parent = AccountName::from(part[0]);
        let entry = self.0.entry(parent.clone()).or_insert(AccountBal {
            name: parent,
            balance: Amount::new(),
            sub_account: None,
        });

        entry.balance += &account.balance;
        Balance::fill_from_top(entry, &part[1..], &account.balance);
    }

    /// Return an Iterator to the parent account of the Balance
    pub fn iter_parent(&self) -> impl Iterator<Item = &AccountBal> {
        self.0.values()
    }

    /// Return an Iterator to the parent account of the Balance
    pub fn iter_parent_mut(&mut self) -> impl Iterator<Item = &mut AccountBal> {
        self.0.values_mut()
    }

    /// Simplifies an account hierarchy by merging sub-accounts where possible.
    ///
    /// A sub-account is merged if:
    /// - The parent account balance equals its single child's balance, and
    /// - The parent and all descendants each have only one child.
    ///
    /// Accounts with multiple children or differing balances remain unchanged.
    ///
    /// ## Examples
    ///
    /// Example 1: can collapse
    /// Assets (100)
    /// |-- Bank (100)
    ///     `-- Checking (100)
    ///
    /// → Collapses into:
    /// Assets:Bank:Checking (100)
    ///
    /// Example 2: cannot collapse due to multiple children
    /// Assets (100)
    /// |-- Bank (100)
    /// |   |-- Checking (80)
    /// |   `-- Savings  (20)
    ///
    /// → Remains unchanged
    ///
    /// Example 3: cannot collapse due to different balances
    /// Assets (50)
    /// |-- Checking (50)
    ///     `-- Savings (20)
    ///
    /// → Remains unchanged
    fn merge_subaccounts(
        curr: &mut AccountBal,
        accn: &AccountName,
        ammount: &Amount,
    ) -> Option<AccountName> {
        let Some(ref mut children) = curr.sub_account else {
            return Some(accn.append(&curr.name));
        };

        match children.len() {
            0 => Some(accn.append(&curr.name)),
            1 => {
                let child = children.values_mut().next().unwrap();
                if curr.balance == child.balance {
                    let name = accn.append(&curr.name);
                    return Balance::merge_subaccounts(child, &name, ammount);
                }

                // stop mergin from curr, and trying from child
                let name = AccountName::from("");
                let amount = child.balance.clone();
                let name = Balance::merge_subaccounts(child, &name, &amount);
                if let Some(ref name) = name {
                    let old = child.name.clone();
                    child.name = name.clone();
                    child.sub_account = None;
                    let child = children.remove(&old).unwrap();
                    children.insert(name.clone(), child);
                }

                return None;
            }
            _ => {
                let renames: Vec<_> = children
                    .values_mut()
                    .filter_map(|child| {
                        Balance::merge_subaccounts(child, accn, ammount).map(|name| {
                            let old = child.name.clone();
                            child.name = name.clone();
                            child.sub_account = None;
                            (old, name)
                        })
                    })
                    .collect();

                for (old, new) in renames {
                    if let Some(v) = children.remove(&old) {
                        children.insert(new, v);
                    }
                }

                return None;
            }
        }
    }

    fn fill_from_top(acc: &mut AccountBal, parts: &[&str], amount: &Amount) {
        if parts.is_empty() {
            return;
        }

        let name = AccountName::from(parts[0]);
        let sub = acc.sub_account.get_or_insert(BTreeMap::new());
        let entry = sub.entry(name.clone()).or_insert(AccountBal {
            name: name,
            balance: Amount::new(),
            sub_account: None,
        });

        entry.balance += amount;
        Balance::fill_from_top(entry, &parts[1..], amount)
    }
}

impl Deref for Balance {
    type Target = BTreeMap<AccountName, AccountBal>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Balance {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amount;
    use crate::quantity;
    use pretty_assertions::assert_eq;
    use rust_decimal::dec;

    #[test]
    fn test_add_account() {
        let mut bal = Balance::new();

        let acc1 = AccountBal {
            name: AccountName::from("Assets:Bank:Cash"),
            balance: quantity!(100, "$").to_amount(),
            sub_account: None,
        };
        let acc2 = AccountBal {
            name: AccountName::from("Assets:Bank:Card"),
            balance: quantity!(50, "$").to_amount(),
            sub_account: None,
        };

        let acc3 = AccountBal {
            name: AccountName::from("Assets:Saving"),
            balance: quantity!(25, "$").to_amount(),
            sub_account: None,
        };

        let acc4 = AccountBal {
            name: AccountName::from("Expenses"),
            balance: quantity!(12.5, "$").to_amount(),
            sub_account: None,
        };

        bal.add_account_bal(&acc1);
        bal.add_account_bal(&acc2);
        bal.add_account_bal(&acc3);
        bal.add_account_bal(&acc4);

        let expected = Balance(BTreeMap::from([
            (
                AccountName::from("Assets"),
                AccountBal {
                    name: AccountName::from("Assets"),
                    balance: quantity!(175, "$").to_amount(),
                    sub_account: Some(BTreeMap::from([
                        (
                            AccountName::from("Bank"),
                            AccountBal {
                                name: AccountName::from("Bank"),
                                balance: quantity!(150, "$").to_amount(),
                                sub_account: Some(BTreeMap::from([
                                    (
                                        AccountName::from("Cash"),
                                        AccountBal {
                                            name: AccountName::from("Cash"),
                                            balance: quantity!(100, "$").to_amount(),
                                            sub_account: None,
                                        },
                                    ),
                                    (
                                        AccountName::from("Card"),
                                        AccountBal {
                                            name: AccountName::from("Card"),
                                            balance: quantity!(50, "$").to_amount(),
                                            sub_account: None,
                                        },
                                    ),
                                ])),
                            },
                        ),
                        (
                            AccountName::from("Saving"),
                            AccountBal {
                                name: AccountName::from("Saving"),
                                balance: quantity!(25, "$").to_amount(),
                                sub_account: None,
                            },
                        ),
                    ])),
                },
            ),
            (
                AccountName::from("Expenses"),
                AccountBal {
                    name: AccountName::from("Expenses"),
                    balance: quantity!(12.5, "$").to_amount(),
                    sub_account: None,
                },
            ),
        ]));

        assert_eq!(bal, expected)
    }

    #[test]
    fn test_to_compact1() {
        let mut bal = Balance::new();

        let acc1 = AccountBal {
            name: AccountName::from("Assets:Bank:Cash"),
            balance: amount!(100, "$"),
            sub_account: None,
        };
        let acc2 = AccountBal {
            name: AccountName::from("Assets:Bank:Card"),
            balance: amount!(50, "$"),
            sub_account: None,
        };

        let acc3 = AccountBal {
            name: AccountName::from("Assets:Saving"),
            balance: amount!(25, "$"),
            sub_account: None,
        };

        let acc4 = AccountBal {
            name: AccountName::from("Expenses"),
            balance: amount!(10, "$"),
            sub_account: None,
        };

        let acc5 = AccountBal {
            name: AccountName::from("Expenses:Grocery"),
            balance: amount!(12.5, "$"),
            sub_account: None,
        };

        let acc6 = AccountBal {
            name: AccountName::from("Expenses:Food:Fav:Fuente Alemana"),
            balance: amount!(20.5, "$"),
            sub_account: None,
        };

        bal.add_account_bal(&acc1);
        bal.add_account_bal(&acc2);
        bal.add_account_bal(&acc3);
        bal.add_account_bal(&acc4);
        bal.add_account_bal(&acc5);
        bal.add_account_bal(&acc6);

        let bal = bal.to_compact();

        let expected = Balance(BTreeMap::from([
            (
                AccountName::from("Assets"),
                AccountBal {
                    name: AccountName::from("Assets"),
                    balance: amount!(100, "$") + amount!(50, "$") + amount!(25, "$"),
                    sub_account: Some(BTreeMap::from([
                        (
                            AccountName::from("Bank"),
                            AccountBal {
                                name: AccountName::from("Bank"),
                                balance: amount!(150, "$"),
                                sub_account: Some(BTreeMap::from([
                                    (
                                        AccountName::from("Cash"),
                                        AccountBal {
                                            name: AccountName::from("Cash"),
                                            balance: quantity!(100, "$").to_amount(),
                                            sub_account: None,
                                        },
                                    ),
                                    (
                                        AccountName::from("Card"),
                                        AccountBal {
                                            name: AccountName::from("Card"),
                                            balance: quantity!(50, "$").to_amount(),
                                            sub_account: None,
                                        },
                                    ),
                                ])),
                            },
                        ),
                        (
                            AccountName::from("Saving"),
                            AccountBal {
                                name: AccountName::from("Saving"),
                                balance: quantity!(25, "$").to_amount(),
                                sub_account: None,
                            },
                        ),
                    ])),
                },
            ),
            (
                AccountName::from("Expenses"),
                AccountBal {
                    name: AccountName::from("Expenses"),
                    balance: amount!(10, "$") + amount!(12.5, "$") + amount!(20.5, "$"),
                    sub_account: Some(BTreeMap::from([
                        (
                            AccountName::from("Grocery"),
                            AccountBal {
                                name: AccountName::from("Grocery"),
                                balance: quantity!(12.5, "$").to_amount(),
                                sub_account: None,
                            },
                        ),
                        (
                            AccountName::from("Food:Fav:Fuente Alemana"),
                            AccountBal {
                                name: AccountName::from("Food:Fav:Fuente Alemana"),
                                balance: quantity!(20.5, "$").to_amount(),
                                sub_account: None,
                            },
                        ),
                    ])),
                },
            ),
        ]));
        assert_eq!(bal, expected);
    }
    #[test]
    fn test_to_compact2() {
        let mut bal = Balance::new();

        let acc1 = AccountBal {
            name: AccountName::from("Expenses"),
            balance: amount!(10, "$"),
            sub_account: None,
        };

        let acc2 = AccountBal {
            name: AccountName::from("Expenses:Food:Fav:Fuente Alemana"),
            balance: amount!(20.5, "$"),
            sub_account: None,
        };

        bal.add_account_bal(&acc1);
        bal.add_account_bal(&acc2);

        let bal = bal.to_compact();
        let expected = Balance(BTreeMap::from([(
            AccountName::from("Expenses"),
            AccountBal {
                name: AccountName::from("Expenses"),
                balance: amount!(10, "$") + amount!(20.5, "$"),
                sub_account: Some(BTreeMap::from([(
                    AccountName::from("Food:Fav:Fuente Alemana"),
                    AccountBal {
                        name: AccountName::from("Food:Fav:Fuente Alemana"),
                        balance: quantity!(20.5, "$").to_amount(),
                        sub_account: None,
                    },
                )])),
            },
        )]));
        assert_eq!(bal, expected);
    }
}
