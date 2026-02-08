use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::mem;
use std::ops::AddAssign;

use crate::account_view::{
    self, AccountView, CompactAccountView, FlatAccountView, HierAccountView, ValuebleAccountView,
};

use crate::balance::Valuation;
use crate::journal::AccName;
use crate::ntypes::{Arithmetic, TsBasket, Valuable, Zero};

/// Represents a collection of `AccountView`
#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct BalanceView<T: AccountView> {
    accnts: BTreeMap<AccName, T>,
}

impl<T> BalanceView<T>
where
    T: ValuebleAccountView,
    T::TsValue: TsBasket<B: Valuable>,
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

impl<T: AccountView> BalanceView<T> {
    pub fn new() -> Self {
        BalanceView {
            accnts: BTreeMap::new(),
        }
    }

    /// Returns the total balance of all accounts.
    pub fn balance(&self) -> T::TsValue {
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
    pub fn to_flat(self) -> BalanceView<FlatAccountView<T::TsValue>> {
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
    pub fn to_hier(self) -> BalanceView<HierAccountView<T::TsValue>> {
        self.into_accounts()
            .map(|a| a.to_hier())
            .fold(BalanceView::new(), |mut bal, acc| {
                bal += acc;
                bal
            })
    }

    /// Converts this balance into a compact hierarchical balance.
    pub fn to_compact(self) -> BalanceView<CompactAccountView<T::TsValue>> {
        let compact = self
            .to_hier()
            .into_accounts()
            .map(|mut a| {
                (
                    a.name().clone(),
                    account_view::utils::merge_sub_accounts(&mut a),
                )
            })
            .collect();

        BalanceView { accnts: compact }
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
            account_view::utils::limit_accounts_depth(acc, depth);
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
            account_view::utils::limit_accounts_depth(acc, depth);
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
            account_view::utils::limit_accounts_depth(acc, depth);
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
        if let Some(entry) = self.accnts.get_mut(&rhs.name()) {
            *entry = account_view::utils::merge_hier_account(mem::take(entry), rhs);
        } else {
            self.accnts.insert(rhs.name().clone(), rhs);
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
        if let Some(entry) = self.accnts.get_mut(&rhs.name()) {
            *entry = account_view::utils::merge_flat_account(mem::take(entry), rhs);
        } else {
            self.accnts.insert(rhs.name().clone(), rhs);
        }
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
