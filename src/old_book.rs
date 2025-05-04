use std::collections::BTreeMap;

use crate::{TickLevel, TickUpdate};

#[derive(Debug, Clone)]
pub struct BTreeOrderBook {
    best_bid: Option<TickLevel>,
    best_ask: Option<TickLevel>,
    bids: BTreeMap<u32, TickLevel>,
    asks: BTreeMap<u32, TickLevel>,
    last_sequence: u64,
    last_bba_update_id: u64,
}

impl BTreeOrderBook {
    pub fn process_tick_update(&mut self, event: &TickUpdate) {
        self.bids.clear();
        self.asks.clear();

        for level in &event.bid_levels {
            self.bids.insert(level.tick, *level);
        }

        for level in &event.ask_levels {
            self.asks.insert(level.tick, *level);
        }

        self.last_sequence = event.sequence_id;

        if event.sequence_id < self.last_bba_update_id {
            return;
        }

        self.update_bba();
    }

    pub fn sequence_id(&self) -> u64 {
        self.last_sequence
    }
}

impl BTreeOrderBook {
    pub fn new() -> Self {
        Self {
            best_bid: None,
            best_ask: None,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_sequence: 0,
            last_bba_update_id: 0,
        }
    }

    #[inline]
    fn update_bba(&mut self) {
        self.best_bid = self.bids.values().next_back().cloned();
        self.best_ask = self.asks.values().next().cloned();
        self.last_bba_update_id = self.last_sequence;
    }
}

impl Default for BTreeOrderBook {
    fn default() -> Self {
        Self::new()
    }
}
