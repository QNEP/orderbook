use std::collections::BTreeMap;

use tabled::{
    Table,
    settings::{Style, panel::Header},
};

use crate::{FloatLevel, TickLevel, TickUpdate, tick::Decimals};

pub const EPSILON: f64 = 1e-15;

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
            assert!(CACHE_SLOTS > CACHE_EMPTY_SLOTS * 2);
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

    pub fn best_bid(&self) -> FloatLevel {
        FloatLevel {
            price: self
                .tick_decimals
                .fast_tick_to_f64(self.bids_0_tick - self.best_bid_i as u32),
            size: self.bids[self.best_bid_i as usize],
        }
    }

    pub fn best_ask(&self) -> FloatLevel {
        FloatLevel {
            price: self
                .tick_decimals
                .fast_tick_to_f64(self.asks_0_tick + self.best_ask_i as u32),
            size: self.asks[self.best_ask_i as usize],
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

        let bids_heap = self.bids_heap.iter().rev().map(|(tick, size)| FloatLevel {
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

        // asks
        let mut new_asks = update.asks();
        if let Some(lowest_ask) = new_asks.next() {
            if lowest_ask.tick < self.asks_0_tick {
                self.rebalance_asks_lower(lowest_ask.tick);
                self.best_ask_i = (lowest_ask.tick - self.asks_0_tick) as u16;
            } else if lowest_ask.tick < self.best_ask_i as u32 + self.asks_0_tick {
                self.best_ask_i = (lowest_ask.tick - self.asks_0_tick) as u16;
            }

            self.insert_ask(lowest_ask);
        };

        for ask in new_asks {
            self.insert_ask(ask);
        }

        self.rebalance_asks_higher_and_update_best();

        // bids
        let mut new_bids = update.bids();
        if let Some(highest_bid) = new_bids.next() {
            if highest_bid.tick > self.bids_0_tick {
                self.rebalance_bids_higher(highest_bid.tick);
                self.best_bid_i = (self.bids_0_tick - highest_bid.tick) as u16;
            } else if highest_bid.tick > self.bids_0_tick - self.best_bid_i as u32 {
                self.best_bid_i = (self.bids_0_tick - highest_bid.tick) as u16;
            }

            self.insert_bid(highest_bid);
        };

        for bid in new_bids {
            self.insert_bid(bid);
        }

        self.rebalance_bids_lower_and_update_best();
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

    fn rebalance_bids_lower_and_update_best(&mut self) {
        if self.bids[self.best_bid_i as usize] > EPSILON {
            return;
        }

        // might be possible to start at best_bid_i as optimization
        for i in 0..CACHE_SLOTS {
            if self.bids[i] > EPSILON {
                self.best_bid_i = i as u16;
                break;
            }
        }

        // rebalance
        if self.best_bid_i > const { CACHE_EMPTY_SLOTS as u16 * 2 } {
            let shift = self.best_bid_i - CACHE_EMPTY_SLOTS as u16;
            self.bids_0_tick -= shift as u32;
            self.best_bid_i -= shift;
            for i in CACHE_EMPTY_SLOTS..(CACHE_SLOTS - shift as usize) {
                self.bids[i] = self.bids[i + shift as usize]
            }

            for i in (CACHE_SLOTS - shift as usize)..CACHE_SLOTS {
                let tick = self.bids_0_tick - i as u32;
                if let Some(sz) = self.bids_heap.get(&tick) {
                    self.bids[i] = *sz;
                    self.bids_heap.remove(&tick);
                } else {
                    self.bids[i] = 0.0;
                }
            }
        }
    }
    fn rebalance_asks_higher_and_update_best(&mut self) {
        if self.asks[self.best_ask_i as usize] > EPSILON {
            return;
        }

        // might be possible to start at best_ask_i as optimization
        for i in 0..CACHE_SLOTS {
            if self.asks[i] > EPSILON {
                self.best_ask_i = i as u16;
                break;
            }
        }

        if self.best_ask_i > const { CACHE_EMPTY_SLOTS as u16 * 2 } {
            let shift = self.best_ask_i - CACHE_EMPTY_SLOTS as u16;
            self.asks_0_tick += shift as u32;
            self.best_ask_i -= shift;

            for i in CACHE_EMPTY_SLOTS..(CACHE_SLOTS - shift as usize) {
                self.asks[i] = self.asks[i + shift as usize]
            }

            for i in (CACHE_SLOTS - shift as usize)..CACHE_SLOTS {
                let tick = self.asks_0_tick + i as u32;
                if let Some(sz) = self.asks_heap.get(&tick) {
                    self.asks[i] = *sz;
                    self.asks_heap.remove(&tick);
                } else {
                    self.asks[i] = 0.0;
                }
            }
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
    fn rebalance_bids_higher(&mut self, highest_tick: u32) {
        debug_assert!(highest_tick > self.bids_0_tick);

        let new_bids_0_tick = highest_tick + CACHE_EMPTY_SLOTS as u32;
        let shift = (new_bids_0_tick - self.bids_0_tick) as usize;

        // rebuild cache
        let i_eviction_start: usize = if shift >= CACHE_SLOTS {
            0
        } else {
            CACHE_SLOTS - shift
        };

        for i in i_eviction_start..CACHE_SLOTS {
            // TODO: can replace with next initialized tick offsets
            if self.bids[i] > EPSILON {
                let tick = self.bids_0_tick - i as u32;
                self.bids_heap
                    .entry(tick)
                    .and_modify(|sz| *sz = self.bids[i])
                    .or_insert(self.bids[i]);

                self.bids[i] = 0.0
            }
        }

        for i in (0..i_eviction_start).rev() {
            self.bids[i + shift] = self.bids[i];
            self.bids[i] = 0.0;
        }

        self.bids_0_tick = new_bids_0_tick;
    }

    /// invariant: lowest_tick < self.asks_0_tick
    ///
    /// enforces invariant: lowest_tick >= asks_0_tick
    #[inline]
    fn rebalance_asks_lower(&mut self, lowest_tick: u32) {
        debug_assert!(lowest_tick < self.asks_0_tick);

        let new_asks_0_tick = lowest_tick.saturating_sub(CACHE_EMPTY_SLOTS as u32);
        let shift = (self.asks_0_tick - new_asks_0_tick) as usize;

        // rebuild cache
        let i_eviction_start: usize = if shift >= CACHE_SLOTS {
            0
        } else {
            CACHE_SLOTS - shift
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

        for i in (0..i_eviction_start).rev() {
            self.asks[i + shift] = self.asks[i];
            self.asks[i] = 0.0;
        }

        self.asks_0_tick = new_asks_0_tick;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(tick: u32, size: f64) -> TickLevel {
        TickLevel { tick, size }
    }

    #[test]
    fn best_ask() {
        let mut book: OrderBook<3, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![tl(2, 5.0)],
            bids: vec![],
        });

        let best_ask = book.best_ask();

        assert_eq!(best_ask.price, 0.02);
        assert_eq!(best_ask.size, 5.0);
    }

    #[test]
    fn best_bid() {
        let mut book: OrderBook<3, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![],
            bids: vec![tl(1, 10.0)],
        });

        let best_bid = book.best_bid();

        assert_eq!(best_bid.price, 0.01);
        assert_eq!(best_bid.size, 10.0);
    }

    #[test]
    fn init() {
        let mut book: OrderBook<3, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![tl(101, 5.0), tl(102, 15.0), tl(103, 25.0)],
            bids: vec![tl(99, 10.0), tl(98, 20.0), tl(97, 30.0)],
        });

        println!("{book:#?}");
        println!("{book}");

        assert_eq!(book.sequence_id, 0);
        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.best_ask_i, 1);
        assert_eq!(book.best_bid_i, 1);
        assert_eq!(book.asks[0], 0.0);
        assert_eq!(book.asks[1], 5.0);
        assert_eq!(book.asks[2], 15.0);
        assert_eq!(book.bids[0], 0.0);
        assert_eq!(book.bids[1], 10.0);
        assert_eq!(book.bids[2], 20.0);
        assert_eq!(book.asks_heap.len(), 1);
        assert_eq!(book.asks_heap.get(&103), Some(&25.0));
        assert_eq!(book.bids_heap.len(), 1);
        assert_eq!(book.bids_heap.get(&97), Some(&30.0));
    }

    #[test]
    fn update_order_book_with_level_removal_and_addition() {
        let mut book: OrderBook<3, 1> = OrderBook::new(2u8.try_into().unwrap());

        let init = TickUpdate {
            sequence_id: 0,
            asks: vec![tl(101, 5.0)],
            bids: vec![tl(99, 10.0)],
        };

        book.process_tick_update(&init);

        println!("{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.asks[1], 5.0); // tick 101
        assert_eq!(book.asks_heap.len(), 0);

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.bids[1], 10.0); // tick 99
        assert_eq!(book.bids_heap.len(), 0);

        let update = TickUpdate {
            sequence_id: 1,
            asks: vec![tl(101, 0.0), tl(102, 15.0)],
            bids: vec![tl(99, 0.0), tl(98, 20.0)],
        };

        book.process_tick_update(&update);

        println!("{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.asks[1], 0.0); // tick 101 (removed)
        assert_eq!(book.asks[2], 15.0); // tick 102 (added)
        assert_eq!(book.asks_heap.len(), 0);

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.bids[1], 0.0); // tick 99 (removed)
        assert_eq!(book.bids[2], 20.0); // tick 98 (added)
        assert_eq!(book.bids_heap.len(), 0);
    }

    #[test]
    fn test_rebalance_bids_higher() {
        let mut book: OrderBook<4, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![],
            bids: vec![tl(99, 10.0), tl(98, 20.0), tl(97, 30.0)],
        });

        println!("Before rebalance:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.bids[1], 10.0); // tick 99
        assert_eq!(book.bids[2], 20.0); // tick 98
        assert_eq!(book.bids[3], 30.0); // tick 97
        assert_eq!(book.bids_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![],
            bids: vec![tl(101, 15.0)],
        });

        println!("After rebalance higher:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.bids_0_tick, 102);
        assert_eq!(book.bids[0], 0.0); // tick 102 (empty)
        assert_eq!(book.bids[1], 15.0); // tick 101 (new)
        assert_eq!(book.bids[2], 0.0); // tick 100 (empty)
        assert_eq!(book.bids[3], 10.0); // tick 99 (shifted)
        assert_eq!(*book.bids_heap.get(&98).unwrap(), 20.0);
        assert_eq!(*book.bids_heap.get(&97).unwrap(), 30.0);
        assert_eq!(book.bids_heap.len(), 2);
    }

    #[test]
    fn test_rebalance_asks_lower() {
        let mut book: OrderBook<4, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![tl(101, 5.0), tl(102, 20.0), tl(103, 30.0)],
            bids: vec![],
        });

        println!("Before rebalance:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.asks[1], 5.0); // tick 101
        assert_eq!(book.asks[2], 20.0); // tick 102
        assert_eq!(book.asks[3], 30.0); // tick 103
        assert_eq!(book.asks_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![tl(99, 15.0)],
            bids: vec![],
        });

        println!("After rebalance lower:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 98);
        assert_eq!(book.asks[0], 0.0); // tick 98 (empty)
        assert_eq!(book.asks[1], 15.0); // tick 99 (new)
        assert_eq!(book.asks[2], 0.0); // tick 100 (empty)
        assert_eq!(book.asks[3], 5.0); // tick 101 (shifted)
        assert_eq!(*book.asks_heap.get(&102).unwrap(), 20.0);
        assert_eq!(*book.asks_heap.get(&103).unwrap(), 30.0);
        assert_eq!(book.asks_heap.len(), 2);
    }

    #[test]
    fn test_rebalance_bids_lower_and_update_best() {
        let mut book: OrderBook<5, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![],
            bids: vec![tl(99, 10.0), tl(98, 20.0), tl(97, 30.0)],
        });

        println!("Initial state:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.bids[1], 10.0); // tick 99
        assert_eq!(book.bids[2], 20.0); // tick 98
        assert_eq!(book.bids[3], 30.0); // tick 97
        assert_eq!(book.bids_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![],
            bids: vec![
                tl(99, 0.0),   // Remove
                tl(98, 0.0),   // Remove
                tl(97, 35.0),  // Update size
                tl(95, 50.0),  // Add new
                tl(86, 100.0), // Add new much lower
            ],
        });

        println!("After rebalance lower:\n{book:#?}");
        println!("{book}");
        assert_eq!(book.bids_0_tick, 98);
        assert_eq!(book.best_bid_i, 1);
        assert_eq!(book.bids[0], 0.0);
        assert_eq!(book.bids[1], 35.0); // tick 97
        assert_eq!(book.bids[2], 0.0);
        assert_eq!(book.bids[3], 50.0); // tick 95
        assert_eq!(*book.bids_heap.get(&86).unwrap(), 100.0);
        assert_eq!(book.bids_heap.len(), 1);
    }

    #[test]
    fn test_rebalance_asks_higher_and_update_best() {
        let mut book: OrderBook<5, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![tl(101, 5.0), tl(102, 20.0), tl(103, 30.0)],
            bids: vec![],
        });

        println!("Initial state:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.asks[1], 5.0); // tick 101
        assert_eq!(book.asks[2], 20.0); // tick 102
        assert_eq!(book.asks[3], 30.0); // tick 103
        assert_eq!(book.asks_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![
                tl(101, 0.0),   // Remove
                tl(102, 0.0),   // Remove
                tl(103, 35.0),  // Update size
                tl(105, 50.0),  // Add new
                tl(114, 100.0), // Add new much higher
            ],
            bids: vec![],
        });

        println!("After rebalance higher:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 102);
        assert_eq!(book.best_ask_i, 1);

        assert_eq!(book.asks[0], 0.0);
        assert_eq!(book.asks[1], 35.0); // tick 103
        assert_eq!(book.asks[2], 0.0);
        assert_eq!(book.asks[3], 50.0); // tick 105
        assert_eq!(*book.asks_heap.get(&114).unwrap(), 100.0);
        assert_eq!(book.asks_heap.len(), 1);
    }

    #[test]
    fn test_new_best_ask_i_lower_without_rebalance() {
        let mut book: OrderBook<4, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![tl(101, 5.0), tl(102, 20.0), tl(103, 30.0)],
            bids: vec![],
        });

        println!("Initial state:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.best_ask_i, 1);
        assert_eq!(book.asks[1], 5.0); // tick 101
        assert_eq!(book.asks[2], 20.0); // tick 102
        assert_eq!(book.asks[3], 30.0); // tick 103
        assert_eq!(book.asks_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![tl(100, 1.0)],
            bids: vec![],
        });

        println!("After rebalance higher:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.best_ask_i, 0);
        assert_eq!(book.asks[0], 1.0); // tick 100
    }

    #[test]
    fn test_new_best_ask_i_higher_without_rebalance() {
        let mut book: OrderBook<4, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![tl(101, 5.0), tl(102, 20.0), tl(103, 30.0)],
            bids: vec![],
        });

        println!("Initial state:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.best_ask_i, 1);
        assert_eq!(book.asks[1], 5.0); // tick 101
        assert_eq!(book.asks[2], 20.0); // tick 102
        assert_eq!(book.asks[3], 30.0); // tick 103
        assert_eq!(book.asks_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![tl(101, 0.0)],
            bids: vec![],
        });

        println!("After rebalance higher:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.asks_0_tick, 100);
        assert_eq!(book.best_ask_i, 2);
        assert_eq!(book.asks[1], 0.0); // tick 101
        assert_eq!(book.asks[2], 20.0); // tick 102
    }

    #[test]
    fn test_new_best_bid_i_higher_without_rebalance() {
        let mut book: OrderBook<4, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![],
            bids: vec![tl(99, 5.0), tl(98, 20.0), tl(97, 30.0)],
        });

        println!("Initial state:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.best_bid_i, 1);
        assert_eq!(book.bids[1], 5.0); // tick 99
        assert_eq!(book.bids[2], 20.0); // tick 98
        assert_eq!(book.bids[3], 30.0); // tick 97
        assert_eq!(book.bids_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![],
            bids: vec![tl(100, 1.0)],
        });

        println!("After update:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.best_bid_i, 0);
        assert_eq!(book.bids[0], 1.0); // tick 100
    }

    #[test]
    fn test_new_best_bid_i_lower_without_rebalance() {
        let mut book: OrderBook<4, 1> = OrderBook::new(2u8.try_into().unwrap());

        book.process_tick_update(&TickUpdate {
            sequence_id: 0,
            asks: vec![],
            bids: vec![tl(99, 5.0), tl(98, 20.0), tl(97, 30.0)],
        });

        println!("Initial state:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.best_bid_i, 1);
        assert_eq!(book.bids[1], 5.0); // tick 99
        assert_eq!(book.bids[2], 20.0); // tick 98
        assert_eq!(book.bids[3], 30.0); // tick 97
        assert_eq!(book.bids_heap.len(), 0);

        book.process_tick_update(&TickUpdate {
            sequence_id: 1,
            asks: vec![],
            bids: vec![tl(99, 0.0)],
        });

        println!("After update:\n{book:#?}");
        println!("{book}");

        assert_eq!(book.bids_0_tick, 100);
        assert_eq!(book.best_bid_i, 2);
        assert_eq!(book.bids[1], 0.0); // tick 99
        assert_eq!(book.bids[2], 20.0); // tick 98
    }
}
