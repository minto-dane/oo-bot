use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use discord_ooh_bot::domain::oo_counter::count_oo_sequences;

fn legacy_count_oo_sequences(text: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0usize;
    let mut count = 0usize;
    while i + 1 < chars.len() {
        let (a, b) = (chars[i], chars[i + 1]);
        if (a == 'お' && b == 'お') || (a == 'オ' && b == 'オ') || (is_ascii_o(a) && is_ascii_o(b))
        {
            count += 1;
            i += 2;
        } else {
            i += 1;
        }
    }
    count
}

#[inline]
fn is_ascii_o(ch: char) -> bool {
    matches!(ch, 'o' | 'O')
}

fn bench_oo_count(c: &mut Criterion) {
    let mut group = c.benchmark_group("oo_count");

    let data_sets = [
        ("mixed_short", "おおooオオabcOOおおoo大これはおおでもない".repeat(200)),
        ("ascii_long", "Oo".repeat(20_000)),
    ];

    for (name, data) in data_sets {
        group.bench_with_input(BenchmarkId::new("legacy", name), &data, |b, input| {
            b.iter(|| legacy_count_oo_sequences(input));
        });
        group.bench_with_input(BenchmarkId::new("new", name), &data, |b, input| {
            b.iter(|| count_oo_sequences(input));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_oo_count);
criterion_main!(benches);
