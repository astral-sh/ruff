use criterion::{
    black_box, criterion_group, criterion_main, measurement::WallTime, BatchSize, Criterion,
};
use ruff_python_parser::StringKind;
use ruff_text_size::TextRange;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

fn benchmark_parser(criterion: &mut Criterion<WallTime>) {
    let mut group = criterion.benchmark_group("parse");

    let s = "\"\"\"Validate length based{ on BIN for major brands:
        https://en.wikipedia.org/wiki/Payment_card_number#Issuer_identification_number_(IIN)\"\"\"";

    // group.bench_with_input("new_string", &s, |b, &s| {
    //     b.iter_batched(
    //         || s.to_string().into_boxed_str(),
    //         |data| {
    //             ruff_python_parser::string::parse_string_literal(
    //                 black_box(data),
    //                 StringKind::String,
    //                 true,
    //                 TextRange::default(),
    //             )
    //         },
    //         BatchSize::SmallInput,
    //     );
    // });
    //
    // group.bench_function("old_string", |b| {
    //     b.iter_batched(
    //         || s.to_string(),
    //         |data| {
    //             ruff_python_parser::old_string::parse_string_literal(
    //                 black_box(&data),
    //                 StringKind::String,
    //                 true,
    //                 TextRange::default(),
    //             )
    //         },
    //         BatchSize::SmallInput,
    //     );
    // });

    let s = "Item {i+1}";

    group.bench_with_input("new_fstring", &s, |b, &s| {
        b.iter_batched(
            || s.to_string().into_boxed_str(),
            |data| {
                ruff_python_parser::string::parse_fstring_literal_element(
                    black_box(data),
                    true,
                    TextRange::default(),
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.bench_function("old_fstring", |b| {
        b.iter_batched(
            || s.to_string(),
            |data| {
                ruff_python_parser::old_string::parse_fstring_literal_element(
                    black_box(&data),
                    true,
                    TextRange::default(),
                )
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(parser, benchmark_parser);
criterion_main!(parser);
