use std::collections::BTreeMap;

use tabled::{
    Table, Tabled,
    settings::{Style, panel::Header},
};
use tick::Decimals;

pub mod lookup_tables;
pub mod old_book;
pub mod tick;

pub const EPSILON: f64 = 1e-15;

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
    pub ask_levels: Vec<TickLevel>, // Vec<T, I> newtype to track invariants like pointer from zerocopy cool idea to mark sorted
    /// invariant: bid_levels must be sorted highest to lowest price
    pub bid_levels: Vec<TickLevel>,
}

impl TickUpdate {
    #[inline]
    pub fn best_bid(&self) -> Option<TickLevel> {
        self.bid_levels.first().copied()
    }
    #[inline]
    pub fn bids(&self) -> impl ExactSizeIterator<Item = TickLevel> {
        self.bid_levels.iter().copied()
    }
    #[inline]
    pub fn best_ask(&self) -> Option<TickLevel> {
        self.ask_levels.first().copied()
    }
    #[inline]
    pub fn asks(&self) -> impl ExactSizeIterator<Item = TickLevel> {
        self.ask_levels.iter().copied()
    }
}

#[derive(Debug, Clone)]
pub struct OrderBook<const CACHE_SLOTS: usize, const CACHE_EMPTY_SLOTS: usize> {
    sequence_id: u64,

    tick_decimals: Decimals,

    asks_0_tick: u32,
    bids_0_tick: u32,

    best_ask_i: u16,
    best_bid_i: u16,

    // invariant: tick index is lowest to highest
    asks: [f64; CACHE_SLOTS],
    // invariant: tick index is highest to lowest
    bids: [f64; CACHE_SLOTS],

    asks_heap: BTreeMap<u32, f64>,
    bids_heap: BTreeMap<u32, f64>,
}

impl<const CACHE_SLOTS: usize, const CACHE_EMPTY_SLOTS: usize> std::fmt::Display
    for OrderBook<CACHE_SLOTS, CACHE_EMPTY_SLOTS>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let asks = self.asks().rev();
        let bids = self.bids();

        let levels = asks.chain(bids);

        let table = Table::new(levels)
            .with(Header::new(format!("OrderBook @ {}", self.sequence_id)))
            .with(Style::modern_rounded())
            .to_string();

        f.write_str(&table)
    }
}

impl<const CACHE_SLOTS: usize, const CACHE_EMPTY_SLOTS: usize>
    OrderBook<CACHE_SLOTS, CACHE_EMPTY_SLOTS>
{
    pub fn new(tick_decimals: Decimals) -> Self {
        const {
            assert!(CACHE_SLOTS < u16::MAX as usize);
            assert!(CACHE_SLOTS > CACHE_EMPTY_SLOTS);
        }

        Self {
            tick_decimals,
            sequence_id: 0,
            asks_0_tick: u32::MAX,
            bids_0_tick: u32::MIN,
            best_ask_i: 0,
            best_bid_i: 0,
            asks: [0.0; CACHE_SLOTS],
            bids: [0.0; CACHE_SLOTS],
            asks_heap: Default::default(),
            bids_heap: Default::default(),
        }
    }

    pub fn asks(&self) -> impl DoubleEndedIterator<Item = FloatLevel> {
        let asks_heap = self.asks_heap.iter().map(|(tick, size)| FloatLevel {
            price: self.tick_decimals.fast_tick_to_f64(*tick),
            size: *size,
        });

        let asks_cache = self
            .asks
            .iter()
            .enumerate()
            .skip(self.best_ask_i as usize)
            .filter_map(|(i, sz)| {
                if *sz < EPSILON {
                    None
                } else {
                    Some(FloatLevel {
                        price: self
                            .tick_decimals
                            .fast_tick_to_f64(self.asks_0_tick + i as u32),
                        size: *sz,
                    })
                }
            });

        asks_cache.chain(asks_heap)
    }

    pub fn bids(&self) -> impl DoubleEndedIterator<Item = FloatLevel> {
        let bids_cache = self
            .bids
            .iter()
            .enumerate()
            .skip(self.best_bid_i as usize)
            .filter_map(|(i, sz)| {
                if *sz < EPSILON {
                    None
                } else {
                    Some(FloatLevel {
                        price: self
                            .tick_decimals
                            .fast_tick_to_f64(self.bids_0_tick - i as u32),
                        size: *sz,
                    })
                }
            });

        let bids_heap = self.bids_heap.iter().map(|(tick, size)| FloatLevel {
            price: self.tick_decimals.fast_tick_to_f64(*tick),
            size: *size,
        });

        bids_cache.chain(bids_heap)
    }

    pub fn sequence_id(&self) -> u64 {
        self.sequence_id
    }

    /// NOTE: update ordering not handled by book. this always updates book
    pub fn process_tick_update(&mut self, update: &TickUpdate) {
        self.sequence_id = update.sequence_id;

        // asks lowest -> highest
        // bids highest -> lowest

        let mut new_asks = update.asks();
        if let Some(lowest_ask) = new_asks.next() {
            if lowest_ask.tick < self.asks_0_tick {
                self.rebalance_asks(lowest_ask.tick);
                self.best_ask_i = (lowest_ask.tick - self.asks_0_tick) as u16;
            }

            self.insert_ask(lowest_ask);
        };

        for ask in new_asks {
            self.insert_ask(ask);
        }

        let mut new_bids = update.bids();
        if let Some(highest_bid) = new_bids.next() {
            if highest_bid.tick > self.bids_0_tick {
                self.rebalance_bids(highest_bid.tick);
                self.best_bid_i = (self.bids_0_tick - highest_bid.tick) as u16;
            }
            self.insert_bid(highest_bid);
        };

        for bid in new_bids {
            self.insert_bid(bid);
        }
    }

    /// invariant: bid tick <= bids_0_tick
    #[inline]
    fn insert_bid(&mut self, bid: TickLevel) {
        debug_assert!(bid.tick <= self.bids_0_tick);

        let i = (self.bids_0_tick - bid.tick) as usize;

        // cache
        if i < CACHE_SLOTS {
            self.bids[i] = bid.size;
        }
        // heap escape - 0 size
        else if bid.size < EPSILON {
            self.bids_heap.remove(&bid.tick);
        }
        // heap escape - upsert
        else {
            self.bids_heap
                .entry(bid.tick)
                .and_modify(|sz| *sz = bid.size)
                .or_insert(bid.size);
        }
    }

    /// invariant: ask tick >= asks_0_tick
    #[inline]
    fn insert_ask(&mut self, ask: TickLevel) {
        debug_assert!(ask.tick >= self.asks_0_tick);

        let i = (ask.tick - self.asks_0_tick) as usize;

        // cache
        if i < CACHE_SLOTS {
            self.asks[i] = ask.size;
        }
        // heap escape - 0 size
        else if ask.size < EPSILON {
            self.asks_heap.remove(&ask.tick);
        }
        // heap escape - upsert
        else {
            self.asks_heap
                .entry(ask.tick)
                .and_modify(|sz| *sz = ask.size)
                .or_insert(ask.size);
        }
    }

    /// invariant: highest_tick > self.bids_0_tick
    ///
    /// enforces invariant: highest_tick <= bids_0_tick
    #[inline]
    fn rebalance_bids(&mut self, highest_tick: u32) {
        debug_assert!(highest_tick > self.bids_0_tick);

        let new_bids_0_tick = highest_tick + CACHE_EMPTY_SLOTS as u32;
        let shift = (new_bids_0_tick - self.bids_0_tick) as usize;

        // rebuild cache
        let i_eviction_start: usize = if shift >= CACHE_SLOTS {
            0
        } else {
            CACHE_SLOTS - 1 - shift
        };

        for i in i_eviction_start..CACHE_SLOTS {
            // TODO: can replace with next initialized tick offsets
            if self.bids[i] > EPSILON {
                let tick = i as u32 + self.bids_0_tick;
                self.bids_heap
                    .entry(tick)
                    .and_modify(|sz| *sz = self.bids[i])
                    .or_insert(self.bids[i]);

                self.bids[i] = 0.0
            }
        }

        self.bids_0_tick = new_bids_0_tick;
    }

    /// invariant: lowest_tick < self.asks_0_tick
    ///
    /// enforces invariant: lowest_tick >= asks_0_tick
    #[inline]
    fn rebalance_asks(&mut self, lowest_tick: u32) {
        debug_assert!(lowest_tick < self.asks_0_tick);

        let new_asks_0_tick = lowest_tick.saturating_sub(CACHE_EMPTY_SLOTS as u32);
        let shift = (self.asks_0_tick - lowest_tick) as usize;

        // rebuild cache
        let i_eviction_start: usize = if shift >= CACHE_SLOTS {
            0
        } else {
            CACHE_SLOTS - 1 - shift
        };

        for i in i_eviction_start..CACHE_SLOTS {
            // TODO: can replace with next initialized tick offsets
            if self.asks[i] > EPSILON {
                let tick = i as u32 + self.asks_0_tick;
                self.asks_heap
                    .entry(tick)
                    .and_modify(|sz| *sz = self.asks[i])
                    .or_insert(self.asks[i]);

                self.asks[i] = 0.0
            }
        }

        self.asks_0_tick = new_asks_0_tick;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TickLevel;

    fn tl(tick: u32, size: f64) -> TickLevel {
        TickLevel { tick, size }
    }

    #[test]
    fn init() {
        let mut book: OrderBook<3, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            ask_levels: vec![tl(101, 5.0), tl(102, 15.0), tl(103, 25.0)],
            bid_levels: vec![tl(99, 10.0), tl(98, 20.0), tl(97, 30.0)],
        });

        println!("{book:#?}");
        println!("{book}");
    }

    #[test]
    fn update() {
        let mut book: OrderBook<8, 2> = OrderBook::new(2u8.try_into().unwrap());

        let init = TickUpdate {
            sequence_id: 0,
            ask_levels: vec![tl(101, 5.0), tl(102, 15.0), tl(103, 25.0)],
            bid_levels: vec![tl(99, 10.0), tl(98, 20.0), tl(97, 30.0)],
        };

        book.process_tick_update(&init);

        println!("{book}");

        let update = TickUpdate {
            sequence_id: 1,
            ask_levels: vec![tl(101, 6.0), tl(103, 0.0), tl(104, 10.0)],
            bid_levels: vec![tl(99, 12.0), tl(97, 0.0), tl(96, 25.0)],
        };

        book.process_tick_update(&update);

        // println!("{book:#?}");
        println!("{book}");
    }
}
