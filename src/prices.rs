#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PriceType {
    Static,
    Floating,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PriceBasis {
    PerUnit,
    Total,
}
