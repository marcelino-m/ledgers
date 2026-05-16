use serde::{Serialize, Serializer};
use std::collections::BTreeMap;
use std::mem;
use std::ops::AddAssign;

use crate::account_view::{
    self, AccountView, CompactAccountView, FlatAccountView, HierAccountView, ValuebleAccountView,
};

use crate::balance::Valuation;
use crate::journal::AccName;
use crate::ntypes::{Arithmetic, TsBasket, Valuable};

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

    /// Returns the account with the given name, or `None` if not found.
    pub fn account(&self, accnt: &AccName) -> Option<&T> {
        self.accnts.get(accnt)
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
    V: Arithmetic + TsBasket,
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
    T: Arithmetic + TsBasket,
{
    /// An empty account is one with a zero balance and no
    /// sub-accounts
    pub fn remove_zero_accounts(&mut self) {
        self.accnts.retain(|_, acc| {
            acc.remove_zero_sub_accounts();
            !acc.is_zero()
        });
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
    T: Arithmetic + TsBasket,
{
    /// An empty account is one with a zero balance and no
    /// sub-accounts
    pub fn remove_empty_accounts(&mut self) {
        self.accnts
            .retain(|_, acc| !acc.balance().is_zero() || acc.sub_accounts().count() > 0);

        self.accnts
            .values_mut()
            .for_each(|acc| acc.remove_zero_sub_accounts());
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
    V: Arithmetic + TsBasket,
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
    V: Arithmetic + TsBasket,
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
    V: Arithmetic + TsBasket,
{
    fn add_assign(&mut self, rhs: FlatAccountView<V>) {
        *self += rhs.to_hier();
    }
}

/// Adds a `FlatAccount` to a `Balance<FlatAccount>`.
impl<V> AddAssign<FlatAccountView<V>> for BalanceView<FlatAccountView<V>>
where
    V: Arithmetic + TsBasket,
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
    V: Arithmetic + TsBasket,
{
    fn add_assign(&mut self, rhs: BalanceView<FlatAccountView<V>>) {
        for acc in rhs.into_accounts() {
            *self += acc;
        }
    }
}

impl<V> AddAssign<BalanceView<HierAccountView<V>>> for BalanceView<HierAccountView<V>>
where
    V: Arithmetic + TsBasket,
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

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;
    use rust_decimal::dec;

    use crate::account_view::{
        AccountView, FlatAccountView, HierAccountView, utils::build_hier_account,
    };

    use crate::amount;
    use crate::amount::Amount;
    use crate::balance::Valuation;
    use crate::balance_view::BalanceView;
    use crate::holdings::{Holdings, Lot};
    use crate::journal::AccName;
    use crate::misc::today;
    use crate::ntypes::TsBasket;
    use crate::quantity::Quantity;
    use crate::symbol::Symbol;
    use crate::tamount;
    use crate::tamount::TAmount;

    fn lot(
        sym: &str,
        qty: rust_decimal::Decimal,
        m: rust_decimal::Decimal,
        h: rust_decimal::Decimal,
        b: rust_decimal::Decimal,
    ) -> Lot {
        let uprice = |q| {
            Amount::from_quantity(Quantity {
                q,
                s: Symbol::new("$"),
            })
        };
        Lot {
            qty: Quantity {
                q: qty,
                s: Symbol::new(sym),
            },
            m_uprice: uprice(m),
            h_uprice: uprice(h),
            b_uprice: uprice(b),
        }
    }

    /// Wraps Holdings in a TAmount at today's date.
    fn th(lots: impl IntoIterator<Item = Lot>) -> TAmount<Holdings> {
        [(today(), Holdings::from_lots(lots))].into_iter().collect()
    }

    /// Builds a HierAccountView with Holdings at a given path.
    fn hier(name: &str, lots: impl IntoIterator<Item = Lot>) -> HierAccountView<TAmount<Holdings>> {
        build_hier_account(AccName::from(name), th(lots)).unwrap()
    }

    #[test]
    fn balance_total_sums_holdings() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );
        bv += hier(
            "Expenses",
            [lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200))],
        );

        let total: Holdings = bv.balance().at(today()).cloned().unwrap();
        let expected = Holdings::from_lots([
            lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100)),
            lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
        ]);
        assert_eq!(total, expected);
    }

    #[test]
    fn to_flat_with_holdings() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets:Bank",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );

        let flat = bv.to_flat();
        assert_eq!(flat.accounts().count(), 1);
        assert!(flat.account(&AccName::from("Assets:Bank")).is_some());
    }

    #[test]
    fn to_compact_with_holdings() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets:Bank",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );

        let compact = bv.to_compact();
        assert_eq!(compact.accounts().count(), 1);
        assert!(compact.account(&AccName::from("Assets")).is_some());
    }

    #[test]
    fn valued_in_market_converts_holdings_to_amount() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets",
            [
                lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100)),
                lot("MSFT", dec!(5), dec!(200), dec!(200), dec!(200)),
            ],
        );

        let valued = bv.valued_in(Valuation::Market);
        // 10*150 + 5*200 = 2500
        let total = valued.balance().at(today()).cloned().unwrap();
        assert_eq!(total, amount!(2500, "$"));
    }

    #[test]
    fn valued_in_basis_converts_holdings_to_amount() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );

        let valued = bv.valued_in(Valuation::Basis);
        // 10*100 = 1000
        let total: Amount = valued.balance().at(today()).cloned().unwrap();
        assert_eq!(total, amount!(1000, "$"));
    }

    #[test]
    fn remove_zero_accounts_with_holdings() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );
        bv += build_hier_account(AccName::from("Equity"), TAmount::<Holdings>::new()).unwrap();

        assert_eq!(bv.accounts().count(), 2);
        bv.remove_zero_accounts();
        assert_eq!(bv.accounts().count(), 1);
        assert!(bv.account(&AccName::from("Assets")).is_some());
    }

    #[test]
    fn limit_depth_with_holdings() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets:Bank:Checking",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );

        let bv = bv.limit_accounts_depth(2);
        let acc = bv.accounts().next().unwrap();
        // depth 1: Assets, depth 2: Bank, Checking is trimmed
        let bank = acc.sub_accounts().next().unwrap();
        assert_eq!(bank.name(), &AccName::from("Bank"));
        assert_eq!(bank.sub_accounts().count(), 0);
    }

    #[test]
    fn hier_limit_depth_zero_returns_unchanged() {
        // depth==0 means no limit: the `return self` branch (line 147) is hit
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets:Bank:Checking",
            [lot("AAPL", dec!(10), dec!(150), dec!(120), dec!(100))],
        );
        let bv2 = bv.limit_accounts_depth(0);
        // All sub-accounts intact
        let acc = bv2.accounts().next().unwrap();
        assert_eq!(acc.name(), &AccName::from("Assets"));
        let bank = acc.sub_accounts().next().unwrap();
        assert_eq!(bank.name(), &AccName::from("Bank"));
        assert_eq!(bank.sub_accounts().count(), 1);
    }

    #[test]
    fn valued_in_quantity_hier_preserves_accounts() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += hier(
            "Assets:Bank",
            [lot("AAPL", dec!(5), dec!(100), dec!(80), dec!(60))],
        );
        bv += hier(
            "Expenses",
            [lot("MSFT", dec!(3), dec!(200), dec!(200), dec!(200))],
        );

        let valued = bv.valued_in(Valuation::Quantity);
        assert_eq!(valued.accounts().count(), 2);
    }

    #[test]
    fn flat_remove_empty_accounts_removes_zero_balances() {
        // "Assets:Checking" contributes a flat account with non-zero balance.
        // A zero HierAccountView contributes a flat account with zero balance.
        let mut bv: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();

        // Non-zero account via to_flat()
        let h1 = build_hier_account(AccName::from("Assets:Checking"), tamount!(100, "$")).unwrap();
        bv += h1;

        // Zero balance account
        let h2 = build_hier_account(AccName::from("Equity"), TAmount::<Amount>::new()).unwrap();
        bv += h2;

        assert_eq!(bv.accounts().count(), 2);
        bv.remove_empty_accounts();
        assert_eq!(bv.accounts().count(), 1);
        assert_eq!(
            bv.accounts().next().unwrap().name(),
            &AccName::from("Assets:Checking")
        );
    }

    #[test]
    fn flat_limit_depth_zero_returns_unchanged() {
        let mut bv: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();
        let h =
            build_hier_account(AccName::from("Assets:Bank:Checking"), tamount!(100, "$")).unwrap();
        bv += h;

        let bv2 = bv.limit_accounts_depth(0);
        assert_eq!(bv2.accounts().count(), 1);
        assert_eq!(
            bv2.accounts().next().unwrap().name(),
            &AccName::from("Assets:Bank:Checking")
        );
    }

    #[test]
    fn flat_limit_depth_nonzero_trims_hierarchy() {
        let mut bv: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();
        let h =
            build_hier_account(AccName::from("Assets:Bank:Checking"), tamount!(100, "$")).unwrap();
        bv += h;

        // Depth 1 should keep only top-level "Assets"
        let bv2 = bv.limit_accounts_depth(1);
        assert_eq!(bv2.accounts().count(), 1);
        assert_eq!(
            bv2.account(&AccName::from("Assets"))
                .unwrap()
                .balance()
                .at(today())
                .cloned()
                .unwrap(),
            amount!(100, "$")
        );
    }

    #[test]
    fn hier_remove_zero_accounts_all_zero_empties_balance() {
        let mut bv: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv += build_hier_account(AccName::from("Assets"), TAmount::<Holdings>::new()).unwrap();
        bv += build_hier_account(AccName::from("Equity"), TAmount::<Holdings>::new()).unwrap();

        assert_eq!(bv.accounts().count(), 2);
        bv.remove_zero_accounts();
        assert_eq!(bv.accounts().count(), 0);
    }

    #[test]
    fn compact_remove_empty_accounts_removes_zero_no_children() {
        let mut bv_hier: BalanceView<HierAccountView<TAmount<Amount>>> = BalanceView::new();
        bv_hier +=
            build_hier_account(AccName::from("Assets:Checking"), tamount!(100, "$")).unwrap();
        bv_hier += build_hier_account(AccName::from("Equity"), TAmount::<Amount>::new()).unwrap();

        let mut compact = bv_hier.to_compact();
        assert_eq!(compact.accounts().count(), 2);
        compact.remove_empty_accounts();
        assert_eq!(compact.accounts().count(), 1);
        assert_eq!(
            compact.accounts().next().unwrap().name(),
            &AccName::from("Assets:Checking")
        );
    }

    #[test]
    fn compact_remove_empty_accounts_keeps_nonzero_sub_account() {
        let mut bv_hier: BalanceView<HierAccountView<TAmount<Amount>>> = BalanceView::new();
        bv_hier +=
            build_hier_account(AccName::from("Assets:Bank:Checking"), tamount!(50, "$")).unwrap();
        bv_hier +=
            build_hier_account(AccName::from("Assets:Bank:Savings"), tamount!(50, "$")).unwrap();

        let mut compact = bv_hier.to_compact();
        compact.remove_empty_accounts();
        // The top-level "Assets" entry (non-zero) should remain.
        assert_eq!(compact.accounts().count(), 1);
        // Sub-accounts (Checking and Savings) are non-zero so they stay.
        let top = compact.accounts().next().unwrap();
        assert_eq!(top.sub_accounts().count(), 2);
    }

    #[test]
    fn compact_limit_depth_zero_returns_unchanged() {
        let mut bv_hier: BalanceView<HierAccountView<TAmount<Amount>>> = BalanceView::new();
        bv_hier +=
            build_hier_account(AccName::from("Assets:Bank:Checking"), tamount!(100, "$")).unwrap();
        bv_hier +=
            build_hier_account(AccName::from("Assets:Bank:Savings"), tamount!(50, "$")).unwrap();

        let compact = bv_hier.to_compact();
        // depth=0 means no limit: everything should be intact
        let bv2 = compact.limit_accounts_depth(0);
        let accs: Vec<_> = bv2.accounts().collect();
        assert_eq!(accs.len(), 1);
        // Assets:Bank compacts to top-level entry with 2 direct children: Checking and Savings
        assert_eq!(accs[0].sub_accounts().count(), 2);
    }

    #[test]
    fn compact_limit_depth_nonzero_trims() {
        let mut bv_hier: BalanceView<HierAccountView<TAmount<Amount>>> = BalanceView::new();
        bv_hier +=
            build_hier_account(AccName::from("Assets:Bank:Checking"), tamount!(100, "$")).unwrap();

        let compact = bv_hier.to_compact();
        // Depth 1 should remove all sub-accounts from the top-level entry.
        let limited = compact.limit_accounts_depth(1);
        let accs: Vec<_> = limited.accounts().collect();
        assert_eq!(accs.len(), 1);
        assert_eq!(accs[0].sub_accounts().count(), 0);
    }

    #[test]
    fn flat_add_assign_hier() {
        let mut bv: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();
        let hier_acc =
            build_hier_account(AccName::from("Assets:Bank"), tamount!(100, "$")).unwrap();
        bv += hier_acc;

        let accs: Vec<_> = bv.accounts().collect();
        assert_eq!(accs.len(), 1);
        assert_eq!(accs[0].name(), &AccName::from("Assets:Bank"));
    }

    #[test]
    fn hier_add_assign_flat() {
        // Obtain a FlatAccountView via the to_flat() conversion.
        let hier_src = build_hier_account(AccName::from("Assets:Bank"), tamount!(50, "$")).unwrap();
        let flat_accs: Vec<_> = hier_src.to_flat();
        assert_eq!(flat_accs.len(), 1);

        let mut bv: BalanceView<HierAccountView<TAmount<Amount>>> = BalanceView::new();
        for flat_acc in flat_accs {
            bv += flat_acc;
        }

        let accs: Vec<_> = bv.accounts().collect();
        assert_eq!(accs.len(), 1);
        assert_eq!(accs[0].name(), &AccName::from("Assets"));
    }

    #[test]
    fn flat_add_assign_flat_merges_same_name() {
        // Create two flat accounts with the same name via to_flat(), then add both.
        let h1 = build_hier_account(AccName::from("Assets"), tamount!(30, "$")).unwrap();
        let h2 = build_hier_account(AccName::from("Assets"), tamount!(20, "$")).unwrap();

        let mut bv: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();
        bv += h1; // adds Assets via HierAccountView AddAssign (flattens to FlatAccountView)
        bv += h2; // merges into existing Assets entry

        let accs: Vec<_> = bv.accounts().collect();
        assert_eq!(accs.len(), 1);
        let bal = accs[0].balance().at(today()).unwrap();
        let q = bal.to_quantity().unwrap();
        assert_eq!(q.q, dec!(50));
    }

    #[test]
    fn balance_view_flat_add_assign_balance_view_flat() {
        let mut bv1: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();
        bv1 += build_hier_account(AccName::from("Assets"), tamount!(100, "$")).unwrap();

        let mut bv2: BalanceView<FlatAccountView<TAmount<Amount>>> = BalanceView::new();
        bv2 += build_hier_account(AccName::from("Expenses"), tamount!(50, "$")).unwrap();

        bv1 += bv2;
        assert_eq!(bv1.accounts().count(), 2);
    }

    #[test]
    fn balance_view_hier_add_assign_balance_view_hier() {
        let mut bv1: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv1 += hier(
            "Assets",
            [lot("AAPL", dec!(5), dec!(100), dec!(80), dec!(60))],
        );

        let mut bv2: BalanceView<HierAccountView<TAmount<Holdings>>> = BalanceView::new();
        bv2 += hier(
            "Expenses",
            [lot("MSFT", dec!(2), dec!(200), dec!(200), dec!(200))],
        );

        bv1 += bv2;
        assert_eq!(bv1.accounts().count(), 2);
        assert_eq!(
            bv1.balance().at(today()),
            Some(&Holdings::from_lots([
                lot("AAPL", dec!(5), dec!(100), dec!(80), dec!(60)),
                lot("MSFT", dec!(2), dec!(200), dec!(200), dec!(200))
            ])),
        );
    }
}
