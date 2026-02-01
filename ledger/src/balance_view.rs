use serde::{ser::SerializeSeq, Serialize, Serializer};
use std::collections::{btree_map::Entry, BTreeMap};
use std::mem;
use std::ops::AddAssign;

use crate::amount::Amount;
use crate::balance::Valuation;
use crate::journal::AccName;
use crate::ntypes::{Arithmetic, TsBasket, Valuable, Zero};
use crate::tamount::TAmount;

/// Provides a specialized projection of a `Account`, allowing
/// the same financial data to be presented in different formats:
/// flat, full hierarchical and compact hierarchical.
pub trait AccountView {
    type TValue: Arithmetic + TsBasket + Zero;

    /// Returns the name of this account.
    fn name(&self) -> &AccName;

    /// Sets the name of this account.
    fn set_name(&mut self, name: AccName);

    /// Returns the balance of the account
    fn balance(&self) -> &Self::TValue;

    /// Returns an iterator over sub-accounts as immutable references.
    fn sub_accounts(&self) -> impl Iterator<Item = &Self>;

    /// Consumes the account and returns an iterator over its sub-accounts.
    fn into_sub_accounts(self) -> impl Iterator<Item = Self>;

    /// Removes all empty sub accounts. An empty account is one with
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
    fn to_flat(self) -> Vec<FlatAccountView<Self::TValue>>
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
    fn to_hier(self) -> HierAccountView<Self::TValue>
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
    fn to_compact(self) -> CompactAccountView<Self::TValue>
    where
        Self: Sized,
    {
        let mut hier = self.to_hier();
        utils::merge_sub_accounts(&mut hier)
    }
}

/// Extension trait for account views with valuable balances.
pub trait ValuebleAccountView: AccountView
where
    Self::TValue: TsBasket<B: Valuable>,
{
    type AccVV: AccountView<TValue: TsBasket<B = Amount>>;

    /// Converts this account view to a valued representation using
    /// the given valuation.
    fn valued_in(&self, v: Valuation) -> Self::AccVV;
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct HierAccountView<T: Arithmetic + TsBasket> {
    name: AccName,
    balance: T,
    #[serde(serialize_with = "utils::values_only")]
    sub_account: BTreeMap<AccName, HierAccountView<T>>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct CompactAccountView<T: Arithmetic + TsBasket> {
    name: AccName,
    balance: T,
    #[serde(serialize_with = "utils::values_only")]
    sub_account: BTreeMap<AccName, CompactAccountView<T>>,
}

#[derive(Debug, PartialEq, Eq, Serialize, Clone, Default)]
pub struct FlatAccountView<T: Arithmetic + TsBasket> {
    acc_name: AccName,
    balance: T,
}

/// Represents a collection of `AccountView`
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct BalanceView<T: AccountView> {
    accnts: BTreeMap<AccName, T>,
}

impl<T> AccountView for FlatAccountView<T>
where
    T: Arithmetic + TsBasket + Zero,
{
    type TValue = T;

    fn name(&self) -> &AccName {
        &self.acc_name
    }

    fn set_name(&mut self, name: AccName) {
        self.acc_name = name;
    }

    fn balance(&self) -> &Self::TValue {
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

impl<T> AccountView for HierAccountView<T>
where
    T: Arithmetic + TsBasket + Zero,
{
    type TValue = T;

    fn name(&self) -> &AccName {
        &self.name
    }

    fn set_name(&mut self, name: AccName) {
        self.name = name;
    }

    fn balance(&self) -> &Self::TValue {
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

impl<T> AccountView for CompactAccountView<T>
where
    T: Arithmetic + TsBasket + Zero,
{
    type TValue = T;

    fn name(&self) -> &AccName {
        &self.name
    }

    fn set_name(&mut self, name: AccName) {
        self.name = name;
    }

    fn balance(&self) -> &Self::TValue {
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

impl<T> ValuebleAccountView for FlatAccountView<T>
where
    T: Arithmetic + TsBasket<B: Valuable> + Zero,
{
    type AccVV = FlatAccountView<TAmount<Amount>>;

    fn valued_in(&self, v: Valuation) -> Self::AccVV {
        FlatAccountView {
            acc_name: self.acc_name.clone(),
            balance: TAmount::from_iter(
                self.balance
                    .iter_baskets()
                    .map(|(d, amt)| (d, amt.valued_in(v))),
            ),
        }
    }
}

impl<T> ValuebleAccountView for HierAccountView<T>
where
    T: Arithmetic + TsBasket<B: Valuable> + Zero,
{
    type AccVV = HierAccountView<TAmount<Amount>>;

    fn valued_in(&self, v: Valuation) -> Self::AccVV {
        HierAccountView {
            name: self.name.clone(),
            balance: TAmount::from_iter(
                self.balance
                    .iter_baskets()
                    .map(|(d, amt)| (d, amt.valued_in(v))),
            ),
            sub_account: self
                .sub_account
                .iter()
                .map(|(name, sub)| (name.clone(), sub.valued_in(v)))
                .collect(),
        }
    }
}

impl<T> ValuebleAccountView for CompactAccountView<T>
where
    T: Arithmetic + TsBasket<B: Valuable> + Zero,
{
    type AccVV = CompactAccountView<TAmount<Amount>>;

    fn valued_in(&self, v: Valuation) -> Self::AccVV {
        CompactAccountView {
            name: self.name.clone(),
            balance: self
                .balance
                .iter_baskets()
                .map(|(d, amt)| (d, amt.valued_in(v)))
                .collect(),
            sub_account: self
                .sub_account
                .iter()
                .map(|(name, sub)| (name.clone(), sub.valued_in(v)))
                .collect(),
        }
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
    pub fn balance(&self) -> T::TValue {
        self.accounts().map(|a| a.balance().clone()).sum()
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
    pub fn to_flat(self) -> BalanceView<FlatAccountView<T::TValue>> {
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
    pub fn to_hier(self) -> BalanceView<HierAccountView<T::TValue>> {
        self.into_accounts()
            .map(|a| a.to_hier())
            .fold(BalanceView::new(), |mut bal, acc| {
                bal += acc;
                bal
            })
    }

    /// Converts this balance into a compact hierarchical balance.
    pub fn to_compact(self) -> BalanceView<CompactAccountView<T::TValue>> {
        // ensure a fully hierarchical first
        // now compact it
        let compact = self
            .to_hier()
            .into_accounts()
            .fold(
                BalanceView::<HierAccountView<T::TValue>>::new(),
                |mut bal, acc| {
                    bal += acc;
                    bal
                },
            )
            .into_accounts()
            .map(|mut a| (a.name.clone(), utils::merge_sub_accounts(&mut a)))
            .collect();

        BalanceView { accnts: compact }
    }
}

impl<T> BalanceView<T>
where
    T: ValuebleAccountView,
    T::TValue: TsBasket<B: Valuable>,
{
    /// Converts this balance to a valued representation using the given valuation.
    ///
    /// Returns `None` if any account cannot be valued.
    pub fn valued_in(&self, v: Valuation) -> BalanceView<T::AccVV> {
        let accnts = self
            .accnts
            .iter()
            .map(|(name, acc)| (name.clone(), acc.valued_in(v)))
            .collect();

        BalanceView { accnts }
    }
}

impl<V> BalanceView<FlatAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
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

impl<T> BalanceView<HierAccountView<T>>
where
    T: Arithmetic + TsBasket + Zero,
{
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
    pub fn limit_accounts_depth(mut self, depth: usize) -> BalanceView<HierAccountView<T>> {
        if depth == 0 {
            return self;
        }

        self.accnts.values_mut().for_each(|acc| {
            utils::limit_accounts_depth(acc, depth);
        });

        self
    }
}

impl<T> BalanceView<CompactAccountView<T>>
where
    T: Arithmetic + TsBasket + Zero,
{
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
    pub fn limit_accounts_depth(self, depth: usize) -> BalanceView<CompactAccountView<T>> {
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
/// or inserting new ones. The balance's layout (whether compact or
/// fully hierarchical) is preserved after the operation.
impl<V> AddAssign<HierAccountView<V>> for BalanceView<HierAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
    fn add_assign(&mut self, rhs: HierAccountView<V>) {
        if let Some(entry) = self.accnts.get_mut(&rhs.name) {
            *entry = utils::merge_hier_account(mem::take(entry), rhs);
        } else {
            self.accnts.insert(rhs.name.clone(), rhs);
        }
    }
}

/// Adds a `HierAccountView` to a `Balance<FlatAccount>`.
impl<V> AddAssign<HierAccountView<V>> for BalanceView<FlatAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
    fn add_assign(&mut self, rhs: HierAccountView<V>) {
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
impl<V> AddAssign<FlatAccountView<V>> for BalanceView<HierAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
    fn add_assign(&mut self, rhs: FlatAccountView<V>) {
        *self += rhs.to_hier();
    }
}

/// Adds a `FlatAccount` to a `Balance<FlatAccount>`.
impl<V> AddAssign<FlatAccountView<V>> for BalanceView<FlatAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
    fn add_assign(&mut self, rhs: FlatAccountView<V>) {
        let entry = self
            .accnts
            .entry(rhs.acc_name.clone())
            .or_insert(FlatAccountView {
                acc_name: rhs.acc_name.clone(),
                balance: V::default(),
            });

        entry.balance += rhs.balance;
    }
}

impl<V> AddAssign<BalanceView<FlatAccountView<V>>> for BalanceView<FlatAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
    fn add_assign(&mut self, rhs: BalanceView<FlatAccountView<V>>) {
        for acc in rhs.into_accounts() {
            *self += acc;
        }
    }
}

impl<V> AddAssign<BalanceView<HierAccountView<V>>> for BalanceView<HierAccountView<V>>
where
    V: Arithmetic + TsBasket + Zero,
{
    fn add_assign(&mut self, rhs: BalanceView<HierAccountView<V>>) {
        for acc in rhs.into_accounts() {
            *self += acc;
        }
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
pub mod utils {

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
    pub fn to_hier<V>(accnt: impl AccountView<TValue = V>) -> Option<HierAccountView<V>>
    where
        V: Arithmetic + TsBasket,
    {
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
    pub fn build_hier_account<V: Arithmetic + TsBasket>(
        mut name: AccName,
        balance: V,
    ) -> Option<HierAccountView<V>> {
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

    fn first_leaft<V: Arithmetic + TsBasket>(
        acc: &mut HierAccountView<V>,
    ) -> &mut HierAccountView<V> {
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
    pub fn merge_sub_accounts<V: Arithmetic + TsBasket>(
        parent: &mut HierAccountView<V>,
    ) -> CompactAccountView<V> {
        let nchild = parent.sub_account.len();
        let bal_eq = parent.balance == parent.sub_account.values().map(|a| a.balance.clone()).sum();

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

    /// Converts a hierarchical account into a compact account. This
    /// assumes that acc is in CompactAccountView format already
    fn hier_to_compact<V: Arithmetic + TsBasket>(
        acc: &HierAccountView<V>,
    ) -> CompactAccountView<V> {
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
    pub fn flatten_account<V>(acc: impl AccountView<TValue = V>) -> Vec<FlatAccountView<V>>
    where
        V: Arithmetic + TsBasket,
    {
        let no_child = acc.sub_accounts().count() == 0;
        let diff_bal =
            acc.balance().clone() - acc.sub_accounts().map(|a| a.balance().clone()).sum::<V>();

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

    // TODO: [VALUE] refactor code to be able this fn to work with
    // HierAccountView<Value>. TValue is no need here
    /// Merges two hierarchical accounts into one. sharing parent
    /// account
    ///
    /// Adds the balances of `right` into `left` and recursively merges
    /// their subaccounts. If a subaccount exists in `right` but not in `left`,
    /// it is inserted into `left`.
    pub(crate) fn merge_hier_account<V>(
        mut left: HierAccountView<V>,
        right: HierAccountView<V>,
    ) -> HierAccountView<V>
    where
        V: Arithmetic + TsBasket,
    {
        left.balance += right.balance.clone();

        for acc in right.into_sub_accounts() {
            let name = acc.name().clone();
            match left.sub_account.entry(name) {
                Entry::Occupied(mut occupied) => {
                    let existing = occupied.get_mut();
                    *existing = merge_hier_account(mem::take(existing), acc);
                }
                Entry::Vacant(vacant) => {
                    vacant.insert(acc);
                }
            }
        }

        left
    }

    pub fn limit_accounts_depth(
        acc: &mut HierAccountView<impl Arithmetic + TsBasket>,
        deep: usize,
    ) {
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

        use crate::balance_view::HierAccountView;
        use crate::misc::today;
        use crate::tamount;

        use crate::{balance_view::FlatAccountView, journal::AccName};

        #[test]
        fn test_nest_account() {
            let acc1 = FlatAccountView {
                acc_name: AccName::from("Assets:Bank:Cash"),
                balance: tamount!(100, "$"),
            };

            let hier = to_hier(acc1);

            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: tamount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank"),
                        balance: tamount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccountView {
                                name: AccName::from("Cash"),
                                balance: tamount!(100, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            assert_eq!(hier, Some(expected.clone()));

            let hier = to_hier(expected.clone()); // a fully hierarchical account remains equal
            assert_eq!(hier, Some(expected));

            let acc = FlatAccountView {
                acc_name: AccName::from("Assets"),
                balance: tamount!(100, "$"),
            };

            let hier = to_hier(acc);

            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: tamount!(100, "$"),
                sub_account: BTreeMap::new(),
            };

            assert_eq!(hier, Some(expected));

            let acc = HierAccountView {
                name: AccName::from("Assets"),
                balance: tamount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank:Personal"),
                        balance: tamount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccountView {
                                name: AccName::from("Cash"),
                                balance: tamount!(100, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: tamount!(100, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank"),
                        balance: tamount!(100, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Personal"),
                            HierAccountView {
                                name: AccName::from("Personal"),
                                balance: tamount!(100, "$"),
                                sub_account: BTreeMap::from([(
                                    AccName::from("Cash"),
                                    HierAccountView {
                                        name: AccName::from("Cash"),
                                        balance: tamount!(100, "$"),
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
            let bal = tamount!(10, "$");

            // Assets $10
            // `-- Bank $10
            //    `-- Cash $10
            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: tamount!(10, "$"),
                sub_account: BTreeMap::from([(
                    AccName::from("Bank"),
                    HierAccountView {
                        name: AccName::from("Bank"),
                        balance: tamount!(10, "$"),
                        sub_account: BTreeMap::from([(
                            AccName::from("Cash"),
                            HierAccountView {
                                name: AccName::from("Cash"),
                                balance: tamount!(10, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        )]),
                    },
                )]),
            };

            assert_eq!(build_hier_account(name, bal), Some(expected));

            let name = AccName::from("Assets");
            let bal = tamount!(10, "$");
            let expected = HierAccountView {
                name: AccName::from("Assets"),
                balance: tamount!(10, "$"),
                sub_account: BTreeMap::new(),
            };
            assert_eq!(build_hier_account(name, bal), Some(expected));
        }

        #[test]
        fn test_merge_sub_account() {
            let name = AccName::from("Assets:Bank:Cash");
            let mut acc = build_hier_account(name, tamount!(100, "$")).unwrap();

            merge_sub_accounts(&mut acc);
            assert_eq!(
                acc,
                HierAccountView {
                    name: AccName::from("Assets:Bank:Cash"),
                    balance: tamount!(100, "$"),
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
                balance: tamount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Grocery"),
                        HierAccountView {
                            name: AccName::from("Grocery"),
                            balance: tamount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                    (
                        AccName::from("Food"),
                        HierAccountView {
                            name: AccName::from("Food"),
                            balance: tamount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccountView {
                                    name: AccName::from("Fav"),
                                    balance: tamount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccountView {
                                            name: AccName::from("Fuente Alemana"),
                                            balance: tamount!(25, "$"),
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
                    balance: tamount!(50, "$"),
                    sub_account: BTreeMap::from([
                        (
                            AccName::from("Grocery"),
                            HierAccountView {
                                name: AccName::from("Grocery"),
                                balance: tamount!(15, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        ),
                        (
                            AccName::from("Food:Fav:Fuente Alemana"),
                            HierAccountView {
                                name: AccName::from("Food:Fav:Fuente Alemana"),
                                balance: tamount!(25, "$"),
                                sub_account: BTreeMap::new(),
                            },
                        ),
                    ]),
                }
            );
        }

        #[test]
        fn test_flatten() {
            // Expenses $10 + $40
            // |-- Grocery $15
            // `-- Food
            //     `-- Fav
            //         `-- Fuente Alemana $25
            let acc = HierAccountView {
                name: AccName::from("Expenses"),
                balance: tamount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Grocery"),
                        HierAccountView {
                            name: AccName::from("Grocery"),
                            balance: tamount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                    (
                        AccName::from("Food"),
                        HierAccountView {
                            name: AccName::from("Food"),
                            balance: tamount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccountView {
                                    name: AccName::from("Fav"),
                                    balance: tamount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccountView {
                                            name: AccName::from("Fuente Alemana"),
                                            balance: tamount!(25, "$"),
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
                    balance: tamount!(10, "$"),
                },
                FlatAccountView {
                    acc_name: AccName::from("Expenses:Food:Fav:Fuente Alemana"),
                    balance: tamount!(25, "$"),
                },
                FlatAccountView {
                    acc_name: AccName::from("Expenses:Grocery"),
                    balance: tamount!(15, "$"),
                },
            ];

            assert_eq!(flatten_account(acc), expected);
        }

        #[test]
        fn test_merge() {
            let acc1 = build_hier_account(AccName::from("Expenses"), tamount!(10, "$")).unwrap();
            let acc2 = build_hier_account(
                AccName::from("Expenses:Food:Fav:Fuente Alemana"),
                tamount!(25, "$"),
            )
            .unwrap();
            let acc3 =
                build_hier_account(AccName::from("Expenses:Grocery"), tamount!(15, "$")).unwrap();

            let merged = merge_hier_account(merge_hier_account(acc1, acc2), acc3);

            let expected = HierAccountView {
                name: AccName::from("Expenses"),
                balance: tamount!(50, "$"),
                sub_account: BTreeMap::from([
                    (
                        AccName::from("Food"),
                        HierAccountView {
                            name: AccName::from("Food"),
                            balance: tamount!(25, "$"),
                            sub_account: BTreeMap::from([(
                                AccName::from("Fav"),
                                HierAccountView {
                                    name: AccName::from("Fav"),
                                    balance: tamount!(25, "$"),
                                    sub_account: BTreeMap::from([(
                                        AccName::from("Fuente Alemana"),
                                        HierAccountView {
                                            name: AccName::from("Fuente Alemana"),
                                            balance: tamount!(25, "$"),
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
                            balance: tamount!(15, "$"),
                            sub_account: BTreeMap::new(),
                        },
                    ),
                ]),
            };

            assert_eq!(merged, expected);
        }
    }
}
