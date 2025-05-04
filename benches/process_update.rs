use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use orderbook::{OrderBook, TickLevel, TickUpdate};

fn tl(tick: u32, size: f64) -> TickLevel {
    TickLevel { tick, size }
}

const MIDPRICE_TICK: u32 = u32::MAX / 2;

fn create_tick_update(side_size: usize) -> TickUpdate {
    let mut ask_levels = Vec::with_capacity(side_size);
    let mut bid_levels = Vec::with_capacity(side_size);

    for i in 0..side_size {
        ask_levels.push(tl(MIDPRICE_TICK + 1 + i as u32, 0.5 + i as f64));
        bid_levels.push(tl(MIDPRICE_TICK - 1 - i as u32, i as f64));
    }

    TickUpdate {
        sequence_id: 0,
        ask_levels,
        bid_levels,
    }
}

fn bench_process_tick_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("process_tick_update");

    // Benchmark for the new OrderBook implementation
    macro_rules! bench_orderbook {
        ($name:expr, $slots:literal, $empty:literal, $init:expr) => {
            group.bench_function($name, move |b| {
                let mut book: OrderBook<$slots, $empty> = OrderBook::new(2u8.try_into().unwrap());
                let update = create_tick_update(20);

                // Apply initial update if needed
                if $init {
                    book.process_tick_update(&update);
                }

                b.iter_batched(
                    || book.clone(),
                    |mut book| {
                        book.process_tick_update(black_box(&update));
                    },
                    BatchSize::SmallInput,
                );
            });
        };
    }

    // Benchmark for the old BTreeOrderBook implementation
    group.bench_function("old_btree_orderbook", |b| {
        let mut book = orderbook::old_book::BTreeOrderBook::new();
        let update = create_tick_update(20);

        book.process_snapshot(black_box(&update));

        b.iter_batched(
            || book.clone(),
            |mut book| {
                book.process_snapshot(black_box(&update));
            },
            BatchSize::SmallInput,
        );
    });

    bench_orderbook!("init slots: 8, empty: 2", 8, 2, false);
    bench_orderbook!("init slots: 32, empty: 4", 32, 4, false);
    bench_orderbook!("update slots: 8, empty: 2", 8, 2, true);
    bench_orderbook!("update slots: 32, empty: 4", 32, 4, true);

    group.finish();
}

criterion_group!(benches, bench_process_tick_update);
criterion_main!(benches);
