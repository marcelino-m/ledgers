#[macro_export]
macro_rules! quantity {
    ($num:literal, $sym:literal) => {
        $crate::commodity::Quantity {
            q: dec!($num),
            s: $crate::symbol::Symbol::new($sym),
        }
    };
}

#[macro_export]
macro_rules! amount {
    ($num:literal, $sym:literal) => {
        ($crate::quantity!($num, $sym)).to_amount()
    };
}
