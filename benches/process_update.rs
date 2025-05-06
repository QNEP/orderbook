use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use orderbook::{OrderBook, TickLevel, TickUpdate, old_book::BTreeOrderBook};

fn tl(tick: u32, size: f64) -> TickLevel {
    TickLevel { tick, size }
}

const MIDPRICE_TICK: u32 = u32::MAX / 2;

fn create_tick_update(side_size: usize, midprice: u32) -> TickUpdate {
    let mut ask_levels = Vec::with_capacity(side_size);
    let mut bid_levels = Vec::with_capacity(side_size);

    for i in 0..side_size {
        ask_levels.push(tl(midprice + 1 + i as u32, 0.5 + i as f64));
        bid_levels.push(tl(midprice - 1 - i as u32, i as f64));
    }

    TickUpdate {
        sequence_id: 0,
        asks: ask_levels,
        bids: bid_levels,
    }
}

fn bench_process_tick_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("process_tick_update");

    // Macro for benchmarking different OrderBook implementations
    macro_rules! bench_orderbook {
        ($name:expr, $constructor:expr) => {
            group.bench_function($name, move |b| {
                let mut book = $constructor;
                let update = create_tick_update(20, MIDPRICE_TICK);
                book.process_tick_update(&update);

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

    // New parametrized OrderBook benchmarks
    bench_orderbook!(
        "update slots: 128, empty: 32",
        OrderBook::<128, 32>::new(2u8.try_into().unwrap())
    );

    // Old BTreeOrderBook benchmark
    bench_orderbook!("old_btree_orderbook", BTreeOrderBook::new());

    group.finish();
}

fn bench_midprice_trend_up(c: &mut Criterion) {
    let mut group = c.benchmark_group("midprice_trend_up");
    let side_size = 20;
    let iterations = 10;
    let jump_size = 16;

    // Macro for benchmarking different OrderBook implementations
    macro_rules! bench_trend_up {
        ($name:expr, $constructor:expr) => {
            group.bench_function($name, move |b| {
                let mut book = $constructor;
                // Initial state
                let initial_update = create_tick_update(side_size, MIDPRICE_TICK);
                book.process_tick_update(&initial_update);

                // Create all updates upfront
                let mut updates = Vec::with_capacity(iterations * 2);
                let mut midprice = MIDPRICE_TICK;
                for _ in 1..=iterations * 2 {
                    midprice += jump_size;
                    updates.push(create_tick_update(side_size, midprice));
                }

                b.iter_batched(
                    || book.clone(),
                    |mut book| {
                        for update in &updates {
                            book.process_tick_update(black_box(update));
                        }
                    },
                    BatchSize::SmallInput,
                );
            });
        };
    }

    // New parametrized OrderBook benchmarks
    bench_trend_up!(
        "midprice_trend_up slots: 128, empty: 32",
        OrderBook::<128, 32>::new(2u8.try_into().unwrap())
    );

    // Old BTreeOrderBook benchmark
    bench_trend_up!("old_btree_midprice_trend_up", BTreeOrderBook::new());

    group.finish();
}

fn bench_midprice_trend_down(c: &mut Criterion) {
    let mut group = c.benchmark_group("midprice_trend_down");
    let side_size = 20;
    let iterations = 10;
    let jump_size = 16;

    // Macro for benchmarking different OrderBook implementations
    macro_rules! bench_trend_down {
        ($name:expr, $constructor:expr) => {
            group.bench_function($name, move |b| {
                let mut book = $constructor;
                let initial_update = create_tick_update(side_size, MIDPRICE_TICK);
                book.process_tick_update(&initial_update);

                // Create all updates upfront
                let mut updates = Vec::with_capacity(iterations * 2);
                let mut midprice = MIDPRICE_TICK;
                for _ in 1..=iterations * 2 {
                    midprice -= jump_size;
                    updates.push(create_tick_update(side_size, midprice));
                }

                b.iter_batched(
                    || book.clone(),
                    |mut book| {
                        for update in &updates {
                            book.process_tick_update(black_box(update));
                        }
                    },
                    BatchSize::SmallInput,
                );
            });
        };
    }

    // New parametrized OrderBook benchmarks
    bench_trend_down!(
        "midprice_trend_down slots: 128, empty: 32",
        OrderBook::<128, 32>::new(2u8.try_into().unwrap())
    );

    // Old BTreeOrderBook benchmark
    bench_trend_down!("old_btree_midprice_trend_down", BTreeOrderBook::new());

    group.finish();
}

fn bench_midprice_volatile(c: &mut Criterion) {
    let mut group = c.benchmark_group("midprice_volatile");
    let side_size = 20;
    let iterations = 10;
    let jump_size = 16;

    // Macro for benchmarking different OrderBook implementations
    macro_rules! bench_volatile {
        ($name:expr, $constructor:expr) => {
            group.bench_function($name, move |b| {
                let mut book = $constructor;
                // Initial state
                let initial_update = create_tick_update(side_size, MIDPRICE_TICK);
                book.process_tick_update(&initial_update);

                // Create all updates upfront
                let mut updates = Vec::with_capacity(iterations * 2);
                let mut midprice = MIDPRICE_TICK;
                let mut direction: i32 = 1;
                for _ in 1..=iterations * 2 {
                    midprice = midprice.wrapping_add((direction * jump_size as i32) as u32);
                    direction *= -1; // Flip direction
                    updates.push(create_tick_update(side_size, midprice));
                }

                b.iter_batched(
                    || book.clone(),
                    |mut book| {
                        for update in &updates {
                            book.process_tick_update(black_box(update));
                        }
                    },
                    BatchSize::SmallInput,
                );
            });
        };
    }

    bench_volatile!(
        "midprice_volatile slots: 128, empty: 32",
        OrderBook::<128, 32>::new(2u8.try_into().unwrap())
    );

    // Old BTreeOrderBook benchmark
    bench_volatile!("old_btree_midprice_volatile", BTreeOrderBook::new());

    group.finish();
}

criterion_group!(
    benches,
    bench_process_tick_update,
    bench_midprice_trend_up,
    bench_midprice_trend_down,
    bench_midprice_volatile
);
criterion_main!(benches);
