use bmp_trace::{
    tracing::{Params, trace},
    types::Bitmap,
};
use criterion::{Criterion, criterion_group, criterion_main};
use std::{hint::black_box as bb, io::Cursor, time::Duration};

fn benchmarks(c: &mut Criterion) {
    let test_files = [
        "tests/head.pbm",
        "tests/writing.pbm",
        "tests/letters.pbm",
        "tests/wheel.pbm",
        "tests/faces.pbm",
        "tests/stanford.pbm",
    ];
    for test_file in test_files {
        let bm_bytes = std::fs::read(test_file).unwrap();
        let bm = Bitmap::read_from(Cursor::new(bm_bytes), 0.5).unwrap();

        c.bench_function(&format!("trace {test_file}"), |b| {
            b.iter(|| {
                trace(bb(&bm), Params::default(), None).unwrap().unwrap();
            })
        });
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(10))
        .measurement_time(Duration::from_secs(30))
        .sample_size(100);
    targets = benchmarks
}
criterion_main!(benches);
