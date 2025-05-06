use tabled::Tabled;

mod book;
pub mod lookup_tables;
pub mod old_book;
pub mod tick;

pub use book::*;

#[derive(Debug, Clone, Copy, Default, Tabled)]
pub struct TickLevel {
    pub tick: u32,
    pub size: f64,
}

#[derive(Debug, Clone, Copy, Default, Tabled)]
pub struct FloatLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone)]
pub struct TickUpdate {
    pub sequence_id: u64,
    /// invariant: ask_levels must be sorted lowest to highest price
    pub asks: Vec<TickLevel>, // Vec<T, I> newtype to track invariants like pointer from zerocopy cool idea to mark sorted
    /// invariant: bid_levels must be sorted highest to lowest price
    pub bids: Vec<TickLevel>,
}

impl TickUpdate {
    #[inline]
    pub fn best_bid(&self) -> Option<TickLevel> {
        self.bids.first().copied()
    }
    #[inline]
    pub fn bids(&self) -> impl ExactSizeIterator<Item = TickLevel> {
        self.bids.iter().copied()
    }
    #[inline]
    pub fn best_ask(&self) -> Option<TickLevel> {
        self.asks.first().copied()
    }
    #[inline]
    pub fn asks(&self) -> impl ExactSizeIterator<Item = TickLevel> {
        self.asks.iter().copied()
    }
}
