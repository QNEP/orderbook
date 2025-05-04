use criterion::{Criterion, black_box, criterion_group, criterion_main};
use orderbook::tick::Decimals;

fn bench_tick_conversion(c: &mut Criterion) {
    let mut group = c.benchmark_group("tick_conversion");

    let decimals: Decimals = 2u8.try_into().unwrap();
    group.bench_function("reference", move |b| {
        b.iter(|| {
            black_box(decimals.reference_tick_to_f64(black_box(1234)));
        });
    });

    group.bench_function("fast", move |b| {
        b.iter(|| {
            black_box(decimals.fast_tick_to_f64(black_box(1234)));
        });
    });

    group.finish();
}

criterion_group!(benches, bench_tick_conversion);
criterion_main!(benches);
