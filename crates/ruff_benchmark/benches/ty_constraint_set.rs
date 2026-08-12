use std::fmt::Write;
use std::path::PathBuf;

use ruff_benchmark::criterion;
use ruff_benchmark::real_world_projects::{
    TY_ECOSYSTEM_PIN, get_project_cache_dir, install_dependencies_to_cache,
};

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use ty_project::metadata::python_version::SupportedPythonVersion;

mod ty_shared;

use ty_shared::{Case, setup_micro_case, setup_micro_case_inner, setup_rayon};

fn setup_micro_case_venv(name: &str, dependencies: &[&str]) -> PathBuf {
    let cache_dir = get_project_cache_dir(name).expect("Failed to get cache directory");
    std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");

    let venv_path = cache_dir.join(".venv");
    install_dependencies_to_cache(
        name,
        dependencies,
        &venv_path,
        SupportedPythonVersion::Py312,
        TY_ECOSYSTEM_PIN,
    )
    .expect("Failed to install dependencies");

    venv_path
}

/// Regression benchmark for many precise arguments constraining the same type variable.
///
/// The parameters are distinct to avoid exercising the `*args` parameter-type accumulator. The
/// important part is that specialization inference should not repeatedly rebuild a growing union
/// for `T` as each argument adds another solution.
fn benchmark_typevar_mapping_large_accumulation(criterion: &mut Criterion) {
    const NUM_ARGUMENTS: usize = 256;

    setup_rayon();

    let mut code = "def combine[T](\n".to_string();
    for i in 0..NUM_ARGUMENTS {
        writeln!(&mut code, "    p{i}: T,").ok();
    }
    code.push_str(") -> T:\n    return p0\n\ncombine(\n");

    for i in 0..NUM_ARGUMENTS {
        writeln!(&mut code, r#"    ("field_{i}", {i}),"#).ok();
    }

    code.push_str(")\n");

    criterion.bench_function("ty_micro[typevar_mapping_accumulation]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

/// Benchmark for many small type-variable accumulations.
///
/// This guards the common case where each type variable only receives a few constraints. Optimizing
/// the large-accumulation case should not make these small generic calls slower.
fn benchmark_typevar_mapping_small_accumulations(criterion: &mut Criterion) {
    const NUM_CALLS: usize = 256;

    setup_rayon();

    let mut code = "\
def combine[T](first: T, second: T, third: T) -> T:
    return first

"
    .to_string();

    for i in 0..NUM_CALLS {
        writeln!(
            &mut code,
            r#"combine(("field_{i}", {i}), ("other_{i}", "{i}"), ("flag_{i}", True))"#
        )
        .ok();
    }

    criterion.bench_function("ty_micro[typevar_mapping_small_accumulations]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

/// Benchmarks solving many union-bearing upper bounds while inferring a generic call.
///
/// Each callable argument places a distinct union upper bound on `T` through callable-parameter
/// contravariance. Fully materializing the conjunction of these bounds would require constructing
/// the cross product of all union alternatives. Factored path bounds and bounded intersection
/// keep the work bounded instead.
fn benchmark_factored_upper_bounds(criterion: &mut Criterion) {
    const NUM_CLAUSES: usize = 12;
    const ALTERNATIVES_PER_CLAUSE: usize = 8;

    setup_rayon();

    let mut code = "from collections.abc import Callable\n\n".to_string();
    for clause in 0..NUM_CLAUSES {
        for alternative in 0..ALTERNATIVES_PER_CLAUSE {
            writeln!(&mut code, "class C{clause}_{alternative}: ...").ok();
        }
    }

    code.push_str("\ndef infer[T](\n");
    for clause in 0..NUM_CLAUSES {
        writeln!(&mut code, "    consumer{clause}: Callable[[T], None],").ok();
    }
    code.push_str(") -> T:\n    raise NotImplementedError\n\n");

    for clause in 0..NUM_CLAUSES {
        write!(&mut code, "def consume{clause}(value: ").ok();
        for alternative in 0..ALTERNATIVES_PER_CLAUSE {
            if alternative > 0 {
                code.push_str(" | ");
            }
            write!(&mut code, "C{clause}_{alternative}").ok();
        }
        code.push_str(") -> None: ...\n");
    }

    code.push_str("\nresult = infer(\n");
    for clause in 0..NUM_CLAUSES {
        writeln!(&mut code, "    consume{clause},").ok();
    }
    code.push_str(")\n");

    criterion.bench_function("ty_micro[factored_upper_bounds]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

/// Guards against quadratic pruning when contravariant callbacks contribute many upper-only bounds.
fn benchmark_many_upper_bound_callbacks(criterion: &mut Criterion) {
    const NUM_CALLBACKS: usize = 1_200;

    setup_rayon();

    let mut code = String::from(
        "from collections.abc import Callable\nfrom typing import Literal\n\ndef accepts[T](\n",
    );
    for i in 0..NUM_CALLBACKS {
        writeln!(&mut code, "    cb{i}: Callable[[T], None],").ok();
    }
    code.push_str(") -> None: ...\n\ndef call_many(\n");
    for i in 0..NUM_CALLBACKS {
        writeln!(&mut code, "    cb{i}: Callable[[Literal[{i}]], None],").ok();
    }
    code.push_str(") -> None:\n    accepts(\n");
    for i in 0..NUM_CALLBACKS {
        writeln!(&mut code, "        cb{i},").ok();
    }
    code.push_str("    )\n");

    criterion.bench_function("ty_micro[many_upper_bound_callbacks]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_pandas_tdd(criterion: &mut Criterion) {
    setup_rayon();
    let venv_path = setup_micro_case_venv("pandas_tdd", &["pandas-stubs"]);
    let code = r#"
        import pandas as pd

        df = pd.DataFrame({
            "a": [1, 2, 3],
            "b": [4, 5, 6],
            "c": [7, 8, 9],
        })
        df["d"] = df["a"] + df["b"] + df["c"] + 1 + (
            df["a"] ** 2 + df["b"] ** 2 + df["c"] ** 2)
        "#;

    // This example was reported in https://github.com/astral-sh/ty/issues/3039.
    criterion.bench_function("ty_micro[pandas_tdd]", |b| {
        b.iter_batched_ref(
            || setup_micro_case_inner(code, Some(&venv_path)),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_mixed_typed_dict_union_copy(criterion: &mut Criterion) {
    const NUM_VARIANTS: usize = 12;

    setup_rayon();

    let mut code = concat!(
        "from collections import ChainMap, OrderedDict, defaultdict\n",
        "from collections.abc import Mapping, MutableMapping\n",
        "from typing import Any, Literal, TypedDict\n\n",
    )
    .to_string();

    for i in 0..NUM_VARIANTS {
        writeln!(
            &mut code,
            "class Item{i}(TypedDict):\n    type: Literal[{i}]"
        )
        .ok();
        if i == 0 {
            code.push_str("    other: Any\n");
        }
        code.push('\n');
    }

    code.push_str("type Item = ");
    for i in 0..NUM_VARIANTS {
        if i > 0 {
            code.push_str(" | ");
        }
        write!(&mut code, "Item{i}").ok();
    }

    code.push_str(
        r#"

def copy_dict(value: Item | dict[str, Any]) -> dict[str, object]:
    return dict(value)

def copy_mapping(value: Item | Mapping[str, Any]) -> dict[str, object]:
    return dict(value)

def copy_mutable_mapping(value: Item | MutableMapping[str, Any]) -> dict[str, object]:
    return dict(value)

def copy_ordered_dict(value: Item | OrderedDict[str, Any]) -> dict[str, object]:
    return dict(value)

def copy_default_dict(value: Item | defaultdict[str, Any]) -> dict[str, object]:
    return dict(value)

def copy_chain_map(value: Item | ChainMap[str, Any]) -> dict[str, object]:
    return dict(value)

def copy_narrowed_mapping(value: Item | Mapping[str, Any]) -> dict[str, object] | None:
    if isinstance(value, dict):
        return dict(value)
    return None
"#,
    );

    criterion.bench_function("ty_micro[mixed_typed_dict_union_copy]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_missing_key_typed_dict_union_copy(criterion: &mut Criterion) {
    const NUM_VARIANTS: usize = 12;

    setup_rayon();

    // Regression benchmark for https://github.com/astral-sh/ty/issues/4176.
    let mut code = "from typing import Literal, NotRequired, TypedDict\n\n".to_string();
    for i in 0..NUM_VARIANTS {
        writeln!(
            &mut code,
            "class Item{i}(TypedDict):\n    kind: Literal[{i}]\n    field_{i}: NotRequired[int]\n"
        )
        .ok();
    }

    code.push_str("type Item = ");
    for i in 0..NUM_VARIANTS {
        if i > 0 {
            code.push_str(" | ");
        }
        write!(&mut code, "Item{i}").ok();
    }

    code.push_str(
        r#"

def copy(value: Item) -> dict[str, object] | None:
    if "missing" in value:
        return dict(value)
    return None
"#,
    );

    criterion.bench_function("ty_micro[missing_key_typed_dict_union_copy]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_recursive_typed_dict_union_contextual_inference(criterion: &mut Criterion) {
    const NUM_BRANCHES: usize = 11;

    setup_rayon();

    // Regression benchmark for https://github.com/astral-sh/ty/issues/3663.
    let mut code = "from typing import Literal, TypedDict\n\n".to_string();
    for i in 0..NUM_BRANCHES {
        writeln!(
            &mut code,
            "class Node{i}(TypedDict):\n    type: Literal['node-{i}']\n    children: list['Node']\n"
        )
        .ok();
    }
    code.push_str("class Leaf(TypedDict):\n    type: Literal['leaf']\n    text: str\n\n");
    code.push_str("type Node = ");
    for i in 0..NUM_BRANCHES {
        if i > 0 {
            code.push_str(" | ");
        }
        write!(&mut code, "Node{i}").ok();
    }
    code.push_str(
        r#" | Leaf

value: list[Node] = [
    {"type": "node-0", "children": [
        {"type": "node-1", "children": [
            {"type": "node-2", "children": [{"type": "leaf", "text": "x"}]},
            {"type": "node-3", "children": [{"type": "leaf", "text": "y"}]},
        ]},
        {"type": "node-4", "children": [
            {"type": "node-5", "children": [{"type": "leaf", "text": "z"}]},
            {"type": "node-6", "children": [{"type": "leaf", "text": "w"}]},
        ]},
    ]},
]
"#,
    );

    criterion.bench_function(
        "ty_micro[recursive_typed_dict_union_contextual_inference]",
        |b| {
            b.iter_batched_ref(
                || setup_micro_case(&code),
                |case| {
                    let Case { db } = case;
                    let result = db.check();
                    assert_eq!(result.len(), 0);
                },
                BatchSize::SmallInput,
            );
        },
    );
}

fn benchmark_invariant_generic_return_union(criterion: &mut Criterion) {
    const NUM_VARIANTS: usize = 21;

    setup_rayon();

    // Regression benchmark for https://github.com/astral-sh/ty/issues/3896.
    let mut code = String::new();
    for i in 0..NUM_VARIANTS {
        writeln!(&mut code, "class M{i}: pass").ok();
    }
    code.push_str("\nAllResults = (\n");
    for i in 0..NUM_VARIANTS {
        if i > 0 {
            code.push_str(" |\n");
        }
        write!(&mut code, "    dict[int, M{i}]").ok();
    }
    code.push_str("\n)\n\nRows = (\n");
    for i in 0..NUM_VARIANTS {
        if i > 0 {
            code.push_str(" |\n");
        }
        write!(&mut code, "    list[tuple[int, M{i}]]").ok();
    }
    code.push_str(
        r#"
)

def map_rows[T](rows: list[tuple[int, T]]) -> dict[int, T]:
    return {}

def perform(rows: Rows) -> AllResults:
    return map_rows(rows)
"#,
    );

    criterion.bench_function("ty_micro[invariant_generic_return_union]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_sequence_literal_union_access(criterion: &mut Criterion) {
    const NUM_LITERALS: usize = 1_200;

    setup_rayon();

    // Regression benchmark for https://github.com/astral-sh/ty/issues/4089.
    let mut code = String::from(
        "from collections.abc import Sequence\nfrom typing import Literal\n\nItem = Literal[\n",
    );
    for i in 0..NUM_LITERALS {
        writeln!(&mut code, "    'value-{i}',").ok();
    }
    code.push_str(
        r#"]

def iterate(items: Sequence[Item]) -> None:
    for item in items:
        pass

def access(items: Sequence[Item]) -> None:
    items[0]
"#,
    );

    criterion.bench_function("ty_micro[sequence_literal_union_access]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_invariant_generic_union_bound(criterion: &mut Criterion) {
    const NUM_ALIASES: usize = 64;

    setup_rayon();

    let mut code =
        String::from("from collections.abc import Iterable\nfrom typing import Literal\n\n");
    for i in 0..NUM_ALIASES {
        writeln!(
            &mut code,
            "type A{i} = Literal[{i}] | int | str | bytes | float"
        )
        .ok();
    }
    code.push_str("\nALIASES = {\n");
    for i in 0..NUM_ALIASES {
        writeln!(&mut code, "    A{i}: {{{i}: A{i}}},").ok();
    }
    code.push_str(
        r#"}

def consume(items: Iterable[object]) -> None: ...

consume(ALIASES.items())
"#,
    );

    criterion.bench_function("ty_micro[invariant_generic_union_bound]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

fn benchmark_many_invariant_typevars(criterion: &mut Criterion) {
    setup_rayon();

    // Regression benchmark for https://github.com/astral-sh/ty/issues/3989.
    let code = r#"
class Invariant[T]:
    x: T

def f[T1, T2, T3, T4, T5, T6, T7, T8, T9, T10](
    box1: Invariant[T1],
    box2: Invariant[T2],
    box3: Invariant[T3],
    box4: Invariant[T4],
    box5: Invariant[T5],
    box6: Invariant[T6],
    box7: Invariant[T7],
    box8: Invariant[T8],
    box9: Invariant[T9],
    box10: Invariant[T10],
) -> None: ...

x = Invariant[int]()
f(x, x, x, x, x, x, x, x, x, x)
"#;

    criterion.bench_function("ty_micro[many_invariant_typevars]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}
fn benchmark_pydantic_core_schema_dict(criterion: &mut Criterion) {
    const NUM_CORE_SCHEMA_VARIANTS: usize = 24;

    setup_rayon();

    // Minimized from the pydantic and hydra-zen ecosystem regressions seen during the
    // SpecializationBuilder pending-constraint-set migration. Pydantic has several empty dict
    // literals with a type context equivalent to `dict[Hashable, core_schema.CoreSchema]`
    // (including `schema.setdefault("metadata", {})` and tagged-union choice tables).
    // `CoreSchema` is a large union of TypedDict schema types; this local `CoreSchema` alias is
    // derived from pydantic-core's real `CoreSchema`, but reduced to enough variants to show the
    // regression quickly. Solving the empty-dict specialization creates one lower-bound constraint
    // per union element for `_VT@dict`. Combined with `_KT@dict = Hashable`,
    // PathAssignments/SequentMap traversal derives cross-typevar facts like
    // `TypedDictSchema <= _VT@dict <= Hashable`. This benchmark tracks the cost until constraint
    // projection / path-bounds solving can avoid that work.
    let mut code = "from collections.abc import Hashable\nfrom typing import Literal, NotRequired, TypedDict\n\n"
        .to_string();
    for i in 0..NUM_CORE_SCHEMA_VARIANTS {
        writeln!(
            &mut code,
            "class Schema{i}(TypedDict):\n    type: Literal['schema-{i}']\n    ref: NotRequired[str]\n    value_{i}: NotRequired[int]\n"
        )
        .ok();
    }
    code.push_str("type CoreSchema = ");
    for i in 0..NUM_CORE_SCHEMA_VARIANTS {
        if i > 0 {
            code.push_str(" | ");
        }
        write!(&mut code, "Schema{i}").ok();
    }
    code.push_str("\n\nchoices: dict[Hashable, CoreSchema] = {}\n");

    criterion.bench_function("ty_micro[pydantic_core_schema_dict]", |b| {
        b.iter_batched_ref(
            || setup_micro_case(&code),
            |case| {
                let Case { db } = case;
                let result = db.check();
                assert_eq!(result.len(), 0);
            },
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(
    constraint_set,
    benchmark_typevar_mapping_large_accumulation,
    benchmark_typevar_mapping_small_accumulations,
    benchmark_factored_upper_bounds,
    benchmark_many_upper_bound_callbacks,
    benchmark_pandas_tdd,
    benchmark_mixed_typed_dict_union_copy,
    benchmark_missing_key_typed_dict_union_copy,
    benchmark_recursive_typed_dict_union_contextual_inference,
    benchmark_invariant_generic_return_union,
    benchmark_sequence_literal_union_access,
    benchmark_invariant_generic_union_bound,
    benchmark_many_invariant_typevars,
    benchmark_pydantic_core_schema_dict,
);
criterion_main!(constraint_set);
