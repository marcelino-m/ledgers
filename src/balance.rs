use regex::Regex;
use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::collections::{BTreeMap, btree_map::Entry};
use std::mem;
use std::ops::AddAssign;

use crate::{
    commodity::{Amount, Valuation},
    journal::AccName,
    ledger::Ledger,
    pricedb::PriceDB,
};

pub trait DefaultLayout {
    const DEFAULT_LAYOUT: Layout;
}

pub trait Account: DefaultLayout {
    /// Creates a new account with the given name.
    ///
    /// The account is initialized empty (no balance and no
    /// subaccounts if this is a hierarchical).
    ///
    /// # Arguments
    /// - `name`: The name of the account, which can later be used as
    ///   a relative or fully qualified name depending on the account structure.
    fn new(name: &str) -> Self;

    /// Creates a new account with the given name and initial balance,
    fn with_balance(name: &str, balance: Amount) -> Self;

    /// Returns the name of this account.
    ///
    /// The name can be either:
    /// - Relative (e.g., `Checking`) — typically when the account is flat.
    /// - Full / fully qualified (e.g., `Assets:Bank:Checking`) — typically when
    ///   the account is hierarchical or compact.
    fn name(&self) -> &AccName;

    /// Returns a mutable reference to the name of this account.
    fn name_mut(&mut self) -> &mut AccName;

    /// Sets the name of this account.
    fn set_name(&mut self, name: AccName) {
        *self.name_mut() = name;
    }

    /// Returns the balance of the account
    fn balance(&self) -> &Amount;

    /// Returns an iterator over the sub-accounts of this account
    fn sub_accounts(&self) -> impl Iterator<Item = &Self>;

    /// Returns an iterator that consumes self and yields sub-accounts
    fn into_sub_accounts(self) -> impl Iterator<Item = Self>;

    /// Converts this account into a flat list of accounts.
    ///
    /// Returns a `Vec<FlatAccount>` where each entry represents a fully
    /// qualified account with its balance, discarding the hierarchical
    /// structure.
    ///
    /// Example:
    /// - Hierarchical:
    ///   Assets
    ///     Bank
    ///       Checking   $100
    ///       Savings    $200
    /// - Flat:
    ///   [
    ///     "Assets:Bank:Checking $100",
    ///     "Assets:Bank:Savings  $200"
    ///   ]
    fn to_flat(self) -> Vec<FlatAccount>
    where
        Self: Sized,
    {
        utils::flatten_account(self)
    }

    /// Converts this account into its full hierarchical representation.
    ///
    /// This method expands the account into a tree structure (`HierAccount`),
    /// where each component of the account name becomes a nested subaccount.
    ///
    /// For example, an account with the name `Assets:Bank:Checking $300` would be
    ///   Assets   $300
    ///    |- Bank     $300
    ///        |- Checking  $300
    ///
    /// The resulting structure preserves the complete hierarchy and balance
    /// information of the original account.
    fn to_hier(self) -> HierAccount
    where
        Self: Sized,
    {
        utils::to_hier(self).unwrap()
    }

    /// Converts this account into a compact hierarchical representation.
    ///
    /// This method first builds the full hierarchical tree (`to_hier`) and then
    /// merges subaccounts into their parent nodes. Balances from child accounts
    /// are aggregated and stored at the parent level, producing a summarized
    /// view of the hierarchy.
    ///
    /// This is useful when the full detail of each subaccount is not required,
    /// and only the aggregated totals per branch are of interest.
    ///
    /// Example:
    /// - Full hierarchy:
    ///   ---------------
    ///   Assets
    ///     Bank
    ///       Checking   $100
    ///       Savings    $200
    ///
    /// - Compact form:
    ///   ---------------
    ///   Assets:Bank   $300
    ///      |-- Checking   $100
    ///      `-- Savings    $200
    fn to_compact(self) -> HierAccount
    where
        Self: Sized,
    {
        let mut hier = self.to_hier();
        utils::merge_sub_accounts(&mut hier);
        hier
    }
}

/// Defines the available layout styles for accounts in a balance report.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
pub enum Layout {
    /// Accounts are listed using their full, non-nested names.
    Flat,
    /// Accounts are nested according to their full structure.
    Hierarchical,
    /// Similar to Hierarchical, but merges accounts with single sub-accounts
    Compact,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
pub struct FlatAccount {
    name: AccName,
    balance: Amount,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone)]
pub struct HierAccount {
    name: AccName,
    balance: Amount,
    #[serde(serialize_with = "utils::values_only")]
    sub_account: BTreeMap<AccName, HierAccount>,
}

/// Represents a collection of accounts with an associated layout.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Balance<T: Account> {
    layout: Layout,
    accnts: BTreeMap<AccName, T>,
}

impl DefaultLayout for FlatAccount {
    const DEFAULT_LAYOUT: Layout = Layout::Flat;
}

impl Account for FlatAccount {
    fn new(name: &str) -> Self {
        FlatAccount {
            name: AccName::from(name.to_owned()),
            balance: Amount::new(),
        }
    }

    fn with_balance(name: &str, balance: Amount) -> Self {
        FlatAccount {
            name: AccName::from(name),
            balance: balance,
        }
    }

    fn name(&self) -> &AccName {
        &self.name
    }

    fn name_mut(&mut self) -> &mut AccName {
        &mut self.name
    }

    fn balance(&self) -> &Amount {
        &self.balance
    }

    fn sub_accounts(&self) -> impl Iterator<Item = &Self> {
        std::iter::empty()
    }

    fn into_sub_accounts(self) -> impl Iterator<Item = Self> {
        std::iter::empty()
    }
}

impl DefaultLayout for HierAccount {
    const DEFAULT_LAYOUT: Layout = Layout::Hierarchical;
}

impl Account for HierAccount {
    fn new(name: &str) -> Self {
        HierAccount {
            name: AccName::from(name.to_owned()),
            balance: Amount::new(),
            sub_account: BTreeMap::new(),
        }
    }

    fn with_balance(name: &str, balance: Amount) -> Self {
        utils::build_hier_account(AccName::from(name.to_owned()), balance).unwrap()
    }

    fn name(&self) -> &AccName {
        &self.name
    }

    fn name_mut(&mut self) -> &mut AccName {
        &mut self.name
    }

    fn balance(&self) -> &Amount {
        &self.balance
    }

    fn sub_accounts(&self) -> impl Iterator<Item = &Self> {
        self.sub_account.values()
    }
    fn into_sub_accounts(self) -> impl Iterator<Item = Self> {
        self.sub_account.into_values()
    }
}

impl<T: Account> Default for Balance<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for FlatAccount {
    fn default() -> Self {
        Self::new("")
    }
}

impl Default for HierAccount {
    fn default() -> Self {
        Self::new("")
    }
}

/// Adds a `HierAccount` to a `Balance<HierAccount>`.
///
/// The account is merged into the balance, updating existing entries
/// or inserting new ones. The balance’s layout (whether compact or
/// fully hierarchical) is preserved after the operation.
impl AddAssign<HierAccount> for Balance<HierAccount> {
    fn add_assign(&mut self, rhs: HierAccount) {
        let curr_layout = self.layout;
        if self.layout == Layout::Compact {
            *self = std::mem::take(self).to_hier();
        }

        if let Some(entry) = self.accnts.get_mut(&rhs.name) {
            *entry = utils::merge(mem::take(entry), rhs);
        } else {
            self.accnts.insert(rhs.name.clone(), rhs);
        }

        if curr_layout == Layout::Compact {
            *self = std::mem::take(self).to_compact();
        }
    }
}

/// Adds a `HierAccount` to a `Balance<FlatAccount>`.
impl AddAssign<HierAccount> for Balance<FlatAccount> {
    fn add_assign(&mut self, rhs: HierAccount) {
        let fltten = rhs.to_flat();
        for facc in fltten {
            *self += facc;
        }
    }
}
/// Adds a `FlatAccount` to a `Balance<HierAccount>`.
///
/// The flat account is incorporated into the hierarchical balance,
/// updating existing entries or creating new ones as needed. The
/// hierarchical layout of the balance is preserved after the operation.
impl AddAssign<FlatAccount> for Balance<HierAccount> {
    fn add_assign(&mut self, rhs: FlatAccount) {
        *self += rhs.to_hier();
    }
}

/// Adds a `FlatAccount` to a `Balance<FlatAccount>`.
impl AddAssign<FlatAccount> for Balance<FlatAccount> {
    fn add_assign(&mut self, rhs: FlatAccount) {
        let entry = self.accnts.entry(rhs.name.clone()).or_insert(FlatAccount {
            name: rhs.name.clone(),
            balance: Amount::new(),
        });

        entry.balance += &rhs.balance;
    }
}

impl<T: Account> Balance<T> {
    /// Creates a new, empty balance.
    ///
    /// The balance is initialized with no accounts and a flat layout.
    pub fn new() -> Balance<T> {
        Balance {
            layout: T::DEFAULT_LAYOUT,
            accnts: BTreeMap::new(),
        }
    }
    /// Returns the total balance of all accounts.
    pub fn balance(&self) -> Amount {
        self.accounts().map(|a| a.balance()).sum()
    }

    /// Returns an iterator over all accounts as immutable references.
    pub fn accounts(&self) -> impl Iterator<Item = &T> {
        self.accnts.values()
    }

    /// Returns an iterator over all accounts as mutable references.
    pub fn mut_accounts(&mut self) -> impl Iterator<Item = &mut T> {
        self.accnts.values_mut()
    }

    /// Consumes the balance and returns an iterator over its accounts.
    pub fn into_accounts(self) -> impl Iterator<Item = T> {
        self.accnts.into_values()
    }

    /// Converts this balance into a flat balance.
    ///
    /// All hierarchical accounts are flattened, resulting in a
    /// `Balance<FlatAccount>` where each account has a fully qualified name.
    pub fn to_flat(self) -> Balance<FlatAccount> {
        self.into_accounts()
            .flat_map(|acc| acc.to_flat())
            .fold(Balance::new(), |mut bal, acc| {
                bal += acc;
                bal
            })
    }

    /// Converts this balance into a fully hierarchical balance.
    ///
    /// Each account is expanded into a hierarchical representation
    /// (`HierAccount`), preserving the full structure.
    pub fn to_hier(self) -> Balance<HierAccount> {
        self.into_accounts()
            .map(|a| a.to_hier())
            .fold(Balance::new(), |mut bal, acc| {
                bal += acc;
                bal
            })
    }

    /// Converts this balance into a compact hierarchical balance.
    pub fn to_compact(self) -> Balance<HierAccount> {
        // ensure a fully hierarchical first
        let hier =
            self.into_accounts()
                .map(|a| a.to_hier())
                .fold(Balance::new(), |mut bal, acc| {
                    bal += acc;
                    bal
                });

        // now compact it
        let compact = hier
            .into_accounts()
            .map(|a: HierAccount| a.to_compact())
            .fold(Balance::new(), |mut bal, acc| {
                bal += acc;
                bal
            });

        compact
    }
}

impl Balance<FlatAccount> {
    /// Remove all accounts with an empty/zero balance
    pub fn remove_empty_accounts(&mut self) {
        self.accnts.retain(|_, acc| !acc.balance().is_zero());
    }
}

impl<T> Serialize for Balance<T>
where
    T: Account + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(self.accounts())
    }
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
pub fn trial_balance<'a>(
    ledger: &'a Ledger,
    mode: Valuation,
    qry: &[Regex],
    price_db: &PriceDB,
) -> Balance<FlatAccount> {
    Balance {
        layout: Layout::Flat,
        accnts: ledger
            .get_accounts()
            .filter(|accnt| qry.is_empty() || qry.iter().any(|r| r.is_match(&accnt.name)))
            .map(|a| FlatAccount {
                name: a.name.clone(),
                balance: match mode {
                    Valuation::Basis => a.book_balance(),
                    Valuation::Quantity => a.balance(),
                    Valuation::Market => a.market_balance(price_db),
                    Valuation::Historical => a.historical_value(price_db),
                },
            })
            .map(|accn| (accn.name.clone(), accn))
            .collect(),
    }
}

/// Helper functions for account manipulations
mod utils {

    use super::*;

    /// Serialize only the values of a BTreeMap
    pub fn values_only<S>(
        map: &BTreeMap<AccName, HierAccount>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(map.len()))?;
        for value in map.values() {
            seq.serialize_element(value)?
        }
        seq.end()
    }

    /// Converts a flat or partially hierarchical account into a fully
    /// hierarchical account.
    pub fn to_hier(accnt: impl Account) -> Option<HierAccount> {
        let name = accnt.name().clone();
        let bal = accnt.balance().clone();
        match build_hier_account(name, bal) {
            Some(mut hier) => {
                let leaft = first_leaft(&mut hier);
                for sub in accnt.into_sub_accounts() {
                    let sh = to_hier(sub).unwrap();
                    leaft.sub_account.insert(sh.name().clone(), sh);
                }

                return Some(hier);
            }
            None => None,
        }
    }

    /// Recursively builds a hierarchical account structure from an account name.
    pub fn build_hier_account(mut name: AccName, balance: Amount) -> Option<HierAccount> {
        let pname = name.pop_parent_account();
        if let Some(pname) = pname {
            return Some(HierAccount {
                name: pname.clone(),
                balance: balance.clone(),
                sub_account: match build_hier_account(name, balance) {
                    Some(acc) => BTreeMap::from([(acc.name.clone(), acc)]),
                    None => BTreeMap::new(),
                },
            });
        }

        None
    }

    fn first_leaft(acc: &mut HierAccount) -> &mut HierAccount {
        if acc.sub_account.is_empty() {
            return acc;
        }

        first_leaft(acc.sub_account.values_mut().next().unwrap())
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
    /// Example 2: cannot colla pse due to multiple children
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
    pub fn merge_sub_accounts(parent: &mut HierAccount) {
        let nchild = parent.sub_account.len();
        let bal_eq = parent.balance == parent.sub_account.values().map(|a| &a.balance).sum();

        if nchild == 1 && bal_eq {
            let sub = mem::take(&mut parent.sub_account)
                .into_values()
                .next()
                .unwrap();

            parent.name = parent.name.append(&sub.name);
            parent.sub_account = sub.sub_account;
            merge_sub_accounts(parent);
        }

        let sub_accnt = mem::take(&mut parent.sub_account);
        parent.sub_account = sub_accnt
            .into_iter()
            .map(|(_, mut accnt)| {
                merge_sub_accounts(&mut accnt);
                accnt
            })
            .map(|accnt| (accnt.name.clone(), accnt))
            .collect();
    }

    /// Flattens an account and adds its flat representation to a
    /// result vector.
    ///
    /// # Example
    /// ```text
    /// Account hierarchy: Assets  $400
    ///                    └─ Bank   $300
    ///                        ├─ Checking $100
    ///                        └─ Savings  $200
    ///
    /// After flattening, res:
    /// [
    ///   "Assets $100",
    ///   "Assets:Bank:Checking $100",
    ///   "Assets:Bank:Savings  $200"
    /// ]
    /// ```
    pub fn flatten_account(acc: impl Account) -> Vec<FlatAccount> {
        let no_child = acc.sub_accounts().count() == 0;
        let diff_bal = acc.balance() - &acc.sub_accounts().map(|a| a.balance()).sum();

        let mut res = Vec::new();
        if no_child {
            res.push(FlatAccount {
                name: acc.name().clone(),
                balance: acc.balance().clone(),
            });
        } else if !diff_bal.is_zero() {
            res.push(FlatAccount {
                name: acc.name().clone(),
                balance: diff_bal,
            });
        }

        let pname = acc.name().clone();
        for mut sub in acc.into_sub_accounts() {
            sub.set_name(pname.append(sub.name()));
            res.extend(flatten_account(sub));
        }

        res
    }

    /// Merges two hierarchical accounts into one. sharing parent
    /// account
    ///
    /// Adds the balances of `right` into `left` and recursively merges
    /// their subaccounts. If a subaccount exists in `right` but not in `left`,
    /// it is inserted into `left`.
    pub fn merge(mut left: HierAccount, right: HierAccount) -> HierAccount {
        left.balance += &right.balance;

        for acc in right.into_sub_accounts() {
            let name = acc.name().clone();
            match left.sub_account.entry(name) {
                Entry::Occupied(mut occupied) => {
                    let existing = occupied.get_mut();
                    *existing = merge(mem::take(existing), acc);
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(acc);
                }
            }
        }

        left
    }

    #[cfg(test)]
    mod test {
        use super::*;

        use std::collections::BTreeMap;

        use pretty_assertions::assert_eq;
        use rust_decimal::dec;

        use crate::amount;
        use crate::balance::HierAccount;
        use crate::quantity;

        use crate::{balance::FlatAccount, journal::AccName};

        #[test]
        fn test_nest_account() {
            let acc1 = FlatAccount {
                name: AccName::from("Assets:Bank:Cash"),
                balance: quantity!(100, "$").to_amount(),
            };

            let hier = to_hier(acc1);

            let expected = HierAccount {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccount {
                        name: AccName::from("Bank"),
                        balance: amount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccount {
                                name: AccName::from("Cash"),
                                balance: amount!(100, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            assert_eq!(hier, Some(expected.clone()));

            let hier = to_hier(expected.clone()); // a fully hierarchical account remaind equal
            assert_eq!(hier, Some(expected));

            let acc = FlatAccount {
                name: AccName::from("Assets"),
                balance: quantity!(100, "$").to_amount(),
            };

            let hier = to_hier(acc);

            let expected = HierAccount {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::new(),
            };

            assert_eq!(hier, Some(expected));

            let acc = HierAccount {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccount {
                        name: AccName::from("Bank:Personal"),
                        balance: amount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccount {
                                name: AccName::from("Cash"),
                                balance: amount!(100, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            let expected = HierAccount {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccount {
                        name: AccName::from("Bank"),
                        balance: amount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Personal"),
                            HierAccount {
                                name: AccName::from("Personal"),
                                balance: amount!(100, "$"),
                                sub_account: BTreeMap::from([(
                                    AccName::from("Cash"),
                                    HierAccount {
                                        name: AccName::from("Cash"),
                                        balance: amount!(100, "$"),
                                        sub_account: BTreeMap::new(),
                                    },
                                )]),
                            },
                        )]),
                    },
                )]),
            };

            let hier = to_hier(acc);
            assert_eq!(hier, Some(expected));
        }

        #[test]
        fn test_build_hier_account() {
            let name = AccName::from("Assets:Bank:Cash");
            let bal = amount!(10, "$");

            // Assets $10
            // `-- Bank $10
            //    `-- Cash $10
            let expected = HierAccount {
                name: AccName::from("Assets"),
                balance: amount!(10, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccount {
                        name: AccName::from("Bank"),
                        balance: amount!(10, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccount {
                                name: AccName::from("Cash"),
                                balance: amount!(10, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            assert_eq!(build_hier_account(name, bal), Some(expected));

            let name = AccName::from("Assets");
            let bal = amount!(10, "$");
            let expected = HierAccount {
                name: AccName::from("Assets"),
                balance: amount!(10, "$"),
                sub_account: BTreeMap::new(),
            };
            assert_eq!(build_hier_account(name, bal), Some(expected));
        }

        #[test]
        fn test_merge_sub_account() {
            let name = AccName::from("Assets:Bank:Cash");
            let mut acc = build_hier_account(name, amount!(100, "$")).unwrap();

            merge_sub_accounts(&mut acc);
            assert_eq!(
                acc,
                HierAccount {
                    name: AccName::from("Assets:Bank:Cash"),
                    balance: amount!(100, "$"),
                    sub_account: BTreeMap::new(),
                }
            );

            // Expenses $50
            // |-- Grocery $15
            // `-- Food
            //     `-- Fav
            //         `-- Fuente Alemana $25

            let mut acc = HierAccount {
                name: AccName::from("Expenses"),
                balance: amount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Grocery"),
                        HierAccount {
                            name: AccName::from("Grocery"),
                            balance: amount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                    (
                        AccName::from("Food"),
                        HierAccount {
                            name: AccName::from("Food"),
                            balance: amount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccount {
                                    name: AccName::from("Fav"),
                                    balance: amount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccount {
                                            name: AccName::from("Fuente Alemana"),
                                            balance: amount!(25, "$"),
                                            sub_account: BTreeMap::new(),
                                        },
                                    )]),
                                },
                            )]),
                        },
                    ),
                ]),
            };

            merge_sub_accounts(&mut acc);

            assert_eq!(
                acc,
                HierAccount {
                    name: AccName::from("Expenses"),
                    balance: amount!(50, "$"),
                    sub_account: BTreeMap::from([
                        (
                            AccName::from("Grocery"),
                            HierAccount {
                                name: AccName::from("Grocery"),
                                balance: amount!(15, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        ),
                        (
                            AccName::from("Food:Fav:Fuente Alemana"),
                            HierAccount {
                                name: AccName::from("Food:Fav:Fuente Alemana"),
                                balance: amount!(25, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        ),
                    ]),
                }
            );
        }

        #[test]
        fn test_flaten() {
            // Expenses $10 + $40
            // |-- Grocery $15
            // `-- Food
            //     `-- Fav
            //         `-- Fuente Alemana $25
            let acc = HierAccount {
                name: AccName::from("Expenses"),
                balance: amount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Grocery"),
                        HierAccount {
                            name: AccName::from("Grocery"),
                            balance: amount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                    (
                        AccName::from("Food"),
                        HierAccount {
                            name: AccName::from("Food"),
                            balance: amount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccount {
                                    name: AccName::from("Fav"),
                                    balance: amount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccount {
                                            name: AccName::from("Fuente Alemana"),
                                            balance: amount!(25, "$"),
                                            sub_account: BTreeMap::new(),
                                        },
                                    )]),
                                },
                            )]),
                        },
                    ),
                ]),
            };

            // Expenses $10 + $40
            // |-- Grocery $15
            // `-- Food
            //     `-- Fav
            //         `-- Fuente Alemana $25
            let expected = vec![
                FlatAccount {
                    name: AccName::from("Expenses"),
                    balance: amount!(10, "$"),
                },
                FlatAccount {
                    name: AccName::from("Expenses:Food:Fav:Fuente Alemana"),
                    balance: amount!(25, "$"),
                },
                FlatAccount {
                    name: AccName::from("Expenses:Grocery"),
                    balance: amount!(15, "$"),
                },
            ];

            assert_eq!(flatten_account(acc), expected);
        }

        #[test]
        fn test_merge() {
            let acc1 = build_hier_account(AccName::from("Expenses"), amount!(10, "$")).unwrap();
            let acc2 = build_hier_account(
                AccName::from("Expenses:Food:Fav:Fuente Alemana"),
                amount!(25, "$"),
            )
            .unwrap();
            let acc3 =
                build_hier_account(AccName::from("Expenses:Grocery"), amount!(15, "$")).unwrap();

            let merged = merge(merge(acc1, acc2), acc3);

            let expected = HierAccount {
                name: AccName::from("Expenses"),
                balance: amount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Food"),
                        HierAccount {
                            name: AccName::from("Food"),
                            balance: amount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccount {
                                    name: AccName::from("Fav"),
                                    balance: amount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccount {
                                            name: AccName::from("Fuente Alemana"),
                                            balance: amount!(25, "$"),
                                            sub_account: BTreeMap::new(),
                                        },
                                    )]),
                                },
                            )]),
                        },
                    ),
                    (
                        AccName::from("Grocery"),
                        HierAccount {
                            name: AccName::from("Grocery"),
                            balance: amount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                ]),
            };

            assert_eq!(merged, expected);
        }
    }
}
