use regex::Regex;
use serde::{Serialize, Serializer, ser::SerializeSeq};
use std::collections::{BTreeMap, btree_map::Entry};
use std::mem;
use std::ops::AddAssign;

use crate::{
    commodity::{Amount, Valuation},
    journal::{AccName, Posting, Xact},
    ledger::Ledger,
    pricedb::PriceDB,
};

/// Provides access to postings for a specific account.
pub trait AccPostingSrc<'a> {
    fn acc_name(&self) -> &AccName;
    fn postings(&self) -> Box<dyn Iterator<Item = &'a Posting> + 'a>;
}

/// An `Account` acts as a container for the set of all debits and
/// credits (postings) made to this specific account across various
/// transactions.
///
/// The account balance can be calculated using different `Valuation``
/// schemes: `Basis`, `Quantity`, or `Market` or `Historical`.
pub struct Account<'a> {
    name: AccName,
    postings: Box<dyn AccPostingSrc<'a> + 'a>,
}

/// Represents a collection of accounts.
#[derive(Default)]
pub struct Balance<'a> {
    accnts: BTreeMap<AccName, Account<'a>>,
}

/// Provides a specialized projection of a `Account`, allowing
/// the same financial data to be presented in different formats:
/// flat, full hierarchical and compact hierarchical.
pub trait AccountView {
    /// Returns the name of this account.
    fn name(&self) -> &AccName;

    /// Sets the name of this account.
    fn set_name(&mut self, name: AccName);

    /// Returns the balance of the account
    fn balance(&self) -> &Amount;

    /// Returns an iterator over sub-accounts as immutable references.
    fn sub_accounts(&self) -> impl Iterator<Item = &Self>;

    /// Consumes the account and returns an iterator over its sub-accounts.
    fn into_sub_accounts(self) -> impl Iterator<Item = Self>;

    /// Removes all emppyt sub accounts.  An empty account is one with
    /// a zero balance and no sub-accounts
    fn remove_empty_accounts(&mut self);

    /// Converts this account into a flat list of accounts.
    ///
    /// Returns a `Vec<FlatAccount>` where each entry represents a fully
    /// qualified account with its balance, discarding the hierarchical
    /// structure.
    ///
    /// Example:
    /// ```text
    /// - Hierarchical:
    ///   Assets
    ///   |-- Bank
    ///        |-- Checking   $100
    ///        |-- Savings    $200
    /// - Flat:
    ///   [
    ///     "Assets:Bank:Checking $100",
    ///     "Assets:Bank:Savings  $200"
    ///   ]
    /// ```
    fn to_flat(self) -> Vec<FlatAccountView>
    where
        Self: Sized,
    {
        utils::flatten_account(self)
    }

    /// Converts this account into its full hierarchical representation.
    ///
    /// This method expands the account into a tree structure (`HierAccountView`),
    /// where each component of the account name becomes a nested subaccount.
    ///
    /// For example, an account with the name `Assets:Bank:Checking $300` would be
    /// ```text
    ///   Assets   $300
    ///    |-- Bank     $300
    ///        |-- Checking  $300
    /// ````
    /// The resulting structure preserves the complete hierarchy and balance
    /// information of the original account.
    fn to_hier(self) -> HierAccountView
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
    /// ```text
    /// - Full hierarchy:
    ///   ---------------
    ///   Assets  $300
    ///    |-- Bank  $300
    ///        |-- Checking   $100
    ///        |-- Savings    $200
    ///
    /// - Compact form:
    ///   ---------------
    ///   Assets:Bank   $300
    ///      |-- Checking   $100
    ///      |-- Savings    $200
    /// ```
    fn to_compact(self) -> CompactAccountView
    where
        Self: Sized,
    {
        let mut hier = self.to_hier();
        utils::merge_sub_accounts(&mut hier)
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct HierAccountView {
    name: AccName,
    balance: Amount,
    #[serde(serialize_with = "utils::values_only")]
    sub_account: BTreeMap<AccName, HierAccountView>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct CompactAccountView {
    name: AccName,
    balance: Amount,
    #[serde(serialize_with = "utils::values_only")]
    sub_account: BTreeMap<AccName, CompactAccountView>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct FlatAccountView {
    acc_name: AccName,
    balance: Amount,
}

/// Represents a collection of `AccountView`
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BalanceView<T: AccountView> {
    accnts: BTreeMap<AccName, T>,
}

impl<'a> Account<'a> {
    /// Creates a new account from the given name and postings.
    pub fn from_postings(name: AccName, ps: impl AccPostingSrc<'a> + 'a) -> Account<'a> {
        Account {
            name,
            postings: Box::new(ps),
        }
    }

    /// Returns the name of this account.
    pub fn name(&self) -> &AccName {
        &self.name
    }

    /// Returns the balance of the account
    pub fn balance(&self, v: Valuation, price_db: &PriceDB) -> Amount {
        self.postings.postings().map(|p| p.value(v, price_db)).sum()
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
    pub fn to_hier(&self, v: Valuation, price_db: &PriceDB) -> HierAccountView {
        let name = self.name().clone();
        let bal = self.balance(v, price_db);
        utils::build_hier_account(name, bal).unwrap()
    }
}

impl<'a> Balance<'a> {
    /// Creates a new, empty balance.
    ///
    /// The balance is initialized with no accounts and a flat layout.
    pub fn new() -> Balance<'a> {
        Self::default()
    }

    /// Creates a new balance from the given ledger and optional regex
    pub fn from_ledger<'b>(ledger: &'b Ledger, qry: &[Regex]) -> Balance<'b> {
        Balance {
            accnts: ledger
                .get_all_posting()
                .filter(|ps| qry.is_empty() || qry.iter().any(|r| r.is_match(ps.acc_name())))
                .map(|ps| {
                    (
                        ps.acc_name().clone(),
                        Account::from_postings(ps.acc_name().clone(), ps),
                    )
                })
                .collect(),
        }
    }

    /// Creates a new balance from the given transaction.
    pub fn from_xact<'b>(xact: &'b Xact) -> Balance<'b> {
        Balance {
            accnts: xact
                .get_all_postings()
                .map(|ps| {
                    (
                        ps.acc_name().clone(),
                        Account::from_postings(ps.acc_name().clone(), ps),
                    )
                })
                .collect(),
        }
    }

    /// Returns the total balance of all accounts.
    pub fn balance(&self, v: Valuation, price_db: &PriceDB) -> Amount {
        self.accounts().map(|a| a.balance(v, price_db)).sum()
    }

    /// Returns an iterator over all accounts as immutable references.
    pub fn accounts(&self) -> impl Iterator<Item = &Account<'a>> {
        self.accnts.values()
    }

    /// Consumes the balance and returns an iterator over its accounts.
    pub fn into_accounts(self) -> impl Iterator<Item = Account<'a>> {
        self.accnts.into_values()
    }

    pub fn to_balance_view(
        &self,
        v: Valuation,
        price_db: &PriceDB,
    ) -> BalanceView<HierAccountView> {
        self.accounts().fold(BalanceView::new(), |mut balv, acc| {
            let hier = acc.to_hier(v, price_db);
            balv += hier;
            balv
        })
    }
}

impl AccountView for FlatAccountView {
    fn name(&self) -> &AccName {
        &self.acc_name
    }

    fn set_name(&mut self, name: AccName) {
        self.acc_name = name;
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

    fn remove_empty_accounts(&mut self) {
        // nothing to do, a flat account has no sub-accounts
    }
}

impl AccountView for HierAccountView {
    fn name(&self) -> &AccName {
        &self.name
    }

    fn set_name(&mut self, name: AccName) {
        self.name = name;
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

    fn remove_empty_accounts(&mut self) {
        self.sub_account
            .retain(|_, acc| !acc.balance().is_zero() || acc.sub_accounts().count() > 0);

        self.sub_account
            .values_mut()
            .for_each(|acc| acc.remove_empty_accounts());
    }
}

impl AccountView for CompactAccountView {
    fn name(&self) -> &AccName {
        &self.name
    }

    fn set_name(&mut self, name: AccName) {
        self.name = name;
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

    fn remove_empty_accounts(&mut self) {
        self.sub_account
            .retain(|_, acc| !acc.balance().is_zero() || acc.sub_accounts().count() > 0);

        self.sub_account
            .values_mut()
            .for_each(|acc| acc.remove_empty_accounts());
    }
}

impl<T: AccountView> Default for BalanceView<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AccountView> BalanceView<T> {
    pub fn new() -> Self {
        BalanceView {
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

    /// Consumes the balance and returns an iterator over its accounts.
    pub fn into_accounts(self) -> impl Iterator<Item = T> {
        self.accnts.into_values()
    }

    /// Converts this balance into a flat balance.
    ///
    /// All hierarchical accounts are flattened, resulting in a
    /// `Balance<FlatAccount>` where each account has a fully qualified name.
    pub fn to_flat(self) -> BalanceView<FlatAccountView> {
        self.into_accounts().flat_map(|acc| acc.to_flat()).fold(
            BalanceView::new(),
            |mut bal, acc| {
                bal += acc;
                bal
            },
        )
    }

    /// Converts this balance into a fully hierarchical balance.
    ///
    /// Each account is expanded into a hierarchical representation
    /// (`HierAccountView`), preserving the full structure.
    pub fn to_hier(self) -> BalanceView<HierAccountView> {
        self.into_accounts()
            .map(|a| a.to_hier())
            .fold(BalanceView::new(), |mut bal, acc| {
                bal += acc;
                bal
            })
    }

    /// Converts this balance into a compact hierarchical balance.
    pub fn to_compact(self) -> BalanceView<CompactAccountView> {
        // ensure a fully hierarchical first
        // now compact it
        let compact = self
            .to_hier()
            .into_accounts()
            .fold(BalanceView::<HierAccountView>::new(), |mut bal, acc| {
                bal += acc;
                bal
            })
            .into_accounts()
            .map(|mut a| (a.name.clone(), utils::merge_sub_accounts(&mut a)))
            .collect();

        BalanceView { accnts: compact }
    }
}

impl BalanceView<FlatAccountView> {
    /// Remove all accounts with an empty/zero balance
    pub fn remove_empty_accounts(&mut self) {
        self.accnts.retain(|_, acc| !acc.balance().is_zero());
    }

    /// Keeps only the parent accounts up to the specified depth. Zero
    /// means no limit.
    pub fn limit_accounts_depth(self, depth: usize) -> Self {
        if depth == 0 {
            return self;
        }

        let mut h = self.to_hier();
        h.accnts.values_mut().for_each(|acc| {
            utils::limit_accounts_depth(acc, depth);
        });

        h.to_flat()
    }
}

impl BalanceView<HierAccountView> {
    /// An empty account is one with a zero balance and no
    /// sub-accounts
    pub fn remove_empty_accounts(&mut self) {
        self.accnts
            .retain(|_, acc| !acc.balance().is_zero() || acc.sub_accounts().count() > 0);

        self.accnts
            .values_mut()
            .for_each(|acc| acc.remove_empty_accounts());
    }

    /// Keeps only the parent accounts up to the specified depth. Zero
    /// means no limit.
    pub fn limit_accounts_depth(mut self, depth: usize) -> BalanceView<HierAccountView> {
        if depth == 0 {
            return self;
        }

        self.accnts.values_mut().for_each(|acc| {
            utils::limit_accounts_depth(acc, depth);
        });

        self
    }
}

impl BalanceView<CompactAccountView> {
    /// An empty account is one with a zero balance and no
    /// sub-accounts
    pub fn remove_empty_accounts(&mut self) {
        self.accnts
            .retain(|_, acc| !acc.balance().is_zero() || acc.sub_accounts().count() > 0);

        self.accnts
            .values_mut()
            .for_each(|acc| acc.remove_empty_accounts());
    }

    /// Keeps only the parent accounts up to the specified depth. Zero
    /// means no limit.
    pub fn limit_accounts_depth(self, depth: usize) -> BalanceView<CompactAccountView> {
        if depth == 0 {
            return self;
        }

        let mut h = self.to_hier();

        h.accnts.values_mut().for_each(|acc| {
            utils::limit_accounts_depth(acc, depth);
        });

        h.to_compact()
    }
}

/// Adds a `HierAccountView` to a `Balance<HierAccountView>`.
///
/// The account is merged into the balance, updating existing entries
/// or inserting new ones. The balance’s layout (whether compact or
/// fully hierarchical) is preserved after the operation.
impl AddAssign<HierAccountView> for BalanceView<HierAccountView> {
    fn add_assign(&mut self, rhs: HierAccountView) {
        if let Some(entry) = self.accnts.get_mut(&rhs.name) {
            *entry = utils::merge(mem::take(entry), rhs);
        } else {
            self.accnts.insert(rhs.name.clone(), rhs);
        }
    }
}

/// Adds a `HierAccountView` to a `Balance<FlatAccount>`.
impl AddAssign<HierAccountView> for BalanceView<FlatAccountView> {
    fn add_assign(&mut self, rhs: HierAccountView) {
        let fltten = rhs.to_flat();
        for facc in fltten {
            *self += facc;
        }
    }
}
/// Adds a `FlatAccount` to a `Balance<HierAccountView>`.
///
/// The flat account is incorporated into the hierarchical balance,
/// updating existing entries or creating new ones as needed. The
/// hierarchical layout of the balance is preserved after the operation.
impl AddAssign<FlatAccountView> for BalanceView<HierAccountView> {
    fn add_assign(&mut self, rhs: FlatAccountView) {
        *self += rhs.to_hier();
    }
}

/// Adds a `FlatAccount` to a `Balance<FlatAccount>`.
impl AddAssign<FlatAccountView> for BalanceView<FlatAccountView> {
    fn add_assign(&mut self, rhs: FlatAccountView) {
        let entry = self
            .accnts
            .entry(rhs.acc_name.clone())
            .or_insert(FlatAccountView {
                acc_name: rhs.acc_name.clone(),
                balance: Amount::new(),
            });

        entry.balance += &rhs.balance;
    }
}

impl<T> Serialize for BalanceView<T>
where
    T: AccountView + Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_seq(self.accounts())
    }
}

/// Helper functions for account manipulations
mod utils {

    use super::*;

    /// Serialize only the values of a BTreeMap
    pub fn values_only<S, V>(map: &BTreeMap<AccName, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        let mut seq = serializer.serialize_seq(Some(map.len()))?;
        for value in map.values() {
            seq.serialize_element(value)?
        }
        seq.end()
    }

    /// Converts a flat or partially hierarchical account into a fully
    /// hierarchical account.
    pub fn to_hier(accnt: impl AccountView) -> Option<HierAccountView> {
        let name = accnt.name().clone();
        let bal = accnt.balance().clone();
        match build_hier_account(name, bal) {
            Some(mut hier) => {
                let leaft = first_leaft(&mut hier);
                for sub in accnt.into_sub_accounts() {
                    let sh = to_hier(sub).unwrap();
                    leaft.sub_account.insert(sh.name().clone(), sh);
                }

                Some(hier)
            }
            None => None,
        }
    }

    /// Recursively builds a hierarchical account structure from an account name.
    pub fn build_hier_account(mut name: AccName, balance: Amount) -> Option<HierAccountView> {
        let pname = name.pop_parent_account();
        if let Some(pname) = pname {
            return Some(HierAccountView {
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

    fn first_leaft(acc: &mut HierAccountView) -> &mut HierAccountView {
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
    pub fn merge_sub_accounts(parent: &mut HierAccountView) -> CompactAccountView {
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
            .into_values()
            .map(|mut accnt| {
                merge_sub_accounts(&mut accnt);
                accnt
            })
            .map(|accnt| (accnt.name.clone(), accnt))
            .collect();

        hier_to_compact(parent)
    }

    /// Converts a hierarchical account into a compact account.  This
    /// assume that acc is in CompactAccountView format already
    fn hier_to_compact(acc: &HierAccountView) -> CompactAccountView {
        CompactAccountView {
            name: acc.name.clone(),
            balance: acc.balance.clone(),
            sub_account: acc
                .sub_account
                .iter()
                .map(|(name, sub)| (name.clone(), hier_to_compact(sub)))
                .collect(),
        }
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
    pub fn flatten_account(acc: impl AccountView) -> Vec<FlatAccountView> {
        let no_child = acc.sub_accounts().count() == 0;
        let diff_bal = acc.balance() - &acc.sub_accounts().map(|a| a.balance()).sum();

        let mut res = Vec::new();
        if no_child {
            res.push(FlatAccountView {
                acc_name: acc.name().clone(),
                balance: acc.balance().clone(),
            });
        } else if !diff_bal.is_zero() {
            res.push(FlatAccountView {
                acc_name: acc.name().clone(),
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
    pub fn merge(mut left: HierAccountView, right: HierAccountView) -> HierAccountView {
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

    pub fn limit_accounts_depth(acc: &mut HierAccountView, deep: usize) {
        if deep == 1 {
            acc.sub_account.clear();
            return;
        }
        for sub in acc.sub_account.values_mut() {
            limit_accounts_depth(sub, deep - 1);
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;

        use std::collections::BTreeMap;

        use pretty_assertions::assert_eq;
        use rust_decimal::dec;

        use crate::amount;
        use crate::balance::HierAccountView;
        use crate::quantity;

        use crate::{balance::FlatAccountView, journal::AccName};

        #[test]
        fn test_nest_account() {
            let acc1 = FlatAccountView {
                acc_name: AccName::from("Assets:Bank:Cash"),
                balance: quantity!(100, "$").to_amount(),
            };

            let hier = to_hier(acc1);

            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank"),
                        balance: amount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccountView {
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

            let acc = FlatAccountView {
                acc_name: AccName::from("Assets"),
                balance: quantity!(100, "$").to_amount(),
            };

            let hier = to_hier(acc);

            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::new(),
            };

            assert_eq!(hier, Some(expected));

            let acc = HierAccountView {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank:Personal"),
                        balance: amount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccountView {
                                name: AccName::from("Cash"),
                                balance: amount!(100, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: amount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank"),
                        balance: amount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Personal"),
                            HierAccountView {
                                name: AccName::from("Personal"),
                                balance: amount!(100, "$"),
                                sub_account: BTreeMap::from([(
                                    AccName::from("Cash"),
                                    HierAccountView {
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
            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: amount!(10, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank"),
                        balance: amount!(10, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccountView {
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
            let expected = HierAccountView {
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
                HierAccountView {
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

            let mut acc = HierAccountView {
                name: AccName::from("Expenses"),
                balance: amount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Grocery"),
                        HierAccountView {
                            name: AccName::from("Grocery"),
                            balance: amount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                    (
                        AccName::from("Food"),
                        HierAccountView {
                            name: AccName::from("Food"),
                            balance: amount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccountView {
                                    name: AccName::from("Fav"),
                                    balance: amount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccountView {
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
                HierAccountView {
                    name: AccName::from("Expenses"),
                    balance: amount!(50, "$"),
                    sub_account: BTreeMap::from([
                        (
                            AccName::from("Grocery"),
                            HierAccountView {
                                name: AccName::from("Grocery"),
                                balance: amount!(15, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        ),
                        (
                            AccName::from("Food:Fav:Fuente Alemana"),
                            HierAccountView {
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
            let acc = HierAccountView {
                name: AccName::from("Expenses"),
                balance: amount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Grocery"),
                        HierAccountView {
                            name: AccName::from("Grocery"),
                            balance: amount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                    (
                        AccName::from("Food"),
                        HierAccountView {
                            name: AccName::from("Food"),
                            balance: amount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccountView {
                                    name: AccName::from("Fav"),
                                    balance: amount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccountView {
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
                FlatAccountView {
                    acc_name: AccName::from("Expenses"),
                    balance: amount!(10, "$"),
                },
                FlatAccountView {
                    acc_name: AccName::from("Expenses:Food:Fav:Fuente Alemana"),
                    balance: amount!(25, "$"),
                },
                FlatAccountView {
                    acc_name: AccName::from("Expenses:Grocery"),
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

            let expected = HierAccountView {
                name: AccName::from("Expenses"),
                balance: amount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Food"),
                        HierAccountView {
                            name: AccName::from("Food"),
                            balance: amount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccountView {
                                    name: AccName::from("Fav"),
                                    balance: amount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccountView {
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
                        HierAccountView {
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
