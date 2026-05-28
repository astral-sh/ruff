//! Project-scale benchmarks for ty's call-hierarchy implementation.
//!
//! Runs `prepare_call_hierarchy` / `call_hierarchy_incoming_calls` /
//! `call_hierarchy_outgoing_calls` against a real-world Python project
//! (anyio) at many positions per category — the only way the per-file
//! rayon scan, the attribute-name prefilter, and the per-callee item
//! caching show their actual scaling behaviour.
//!
//! Position selection is deterministic: `document_symbols` is run over
//! every `.py` file under the project's checked paths, symbols are
//! bucketed by category (function / method / class / property / dunder),
//! and each bucket is capped to a fixed size after sorting by
//! `(file_path, name_range.start)` so the same positions are picked across
//! runs.

use criterion::{BatchSize, Criterion, SamplingMode, criterion_group, criterion_main};
use rayon::ThreadPoolBuilder;
use ruff_benchmark::criterion;
use ruff_benchmark::real_world_projects::{InstalledProject, RealWorldProject};
use ruff_db::files::File;
use ruff_db::system::{InMemorySystem, MemoryFileSystem, SystemPath, SystemPathBuf, TestSystem};
use ruff_text_size::TextSize;
use ty_ide::SymbolKind;
use ty_project::metadata::options::{EnvironmentOptions, Options};
use ty_project::metadata::python_version::SupportedPythonVersion;
use ty_project::metadata::value::RangedValue;
use ty_project::{Db, ProjectDatabase, ProjectMetadata};

/// Cap per category. Anyio has plenty of functions/methods, fewer
/// properties; the harvest is `take(MAX_PER_CATEGORY)` after sort so
/// smaller buckets just produce smaller iter loops — the bench scales
/// down gracefully when a project doesn't have N of some category.
const MAX_PER_CATEGORY: usize = 50;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
enum Category {
    Function,
    Method,
    Class,
    Property,
    Dunder,
}

impl Category {
    fn as_str(self) -> &'static str {
        match self {
            Category::Function => "function",
            Category::Method => "method",
            Category::Class => "class",
            Category::Property => "property",
            Category::Dunder => "dunder",
        }
    }

    fn classify(name: &str, kind: SymbolKind) -> Option<Self> {
        let is_dunder = name.len() >= 4 && name.starts_with("__") && name.ends_with("__");
        match (kind, is_dunder) {
            (SymbolKind::Method, true) => Some(Category::Dunder),
            (SymbolKind::Function, true) => Some(Category::Dunder),
            (SymbolKind::Function, false) => Some(Category::Function),
            (SymbolKind::Method, false) => Some(Category::Method),
            (SymbolKind::Class, _) => Some(Category::Class),
            (SymbolKind::Property, _) => Some(Category::Property),
            _ => None,
        }
    }
}

/// One position in the corpus: the cursor offset of a symbol name within
/// its file. Used as the input to `prepare_call_hierarchy` and friends.
#[derive(Clone)]
struct Position {
    file: File,
    offset: TextSize,
}

/// Per-bench fixture: a fully-warm `ProjectDatabase` plus the harvested
/// positions for each category. The db is built once for the whole bench
/// run because each iteration only reads it (no cache invalidation), and
/// reusing it lets criterion run thousands of iters without paying the
/// project-loading cost more than once.
struct Fixture {
    db: ProjectDatabase,
    by_category: std::collections::HashMap<Category, Vec<Position>>,
}

fn setup_anyio() -> InstalledProject<'static> {
    RealWorldProject {
        name: "anyio",
        repository: "https://github.com/agronholm/anyio",
        commit: "561d81270a12f7c6bbafb5bc5fad99a2a13f96be",
        paths: &["src"],
        dependencies: &[],
        max_dep_date: "2025-06-17",
        python_version: SupportedPythonVersion::Py313,
    }
    .setup()
    .expect("Failed to setup anyio")
}

fn build_db(project: &InstalledProject<'_>) -> ProjectDatabase {
    let fs: MemoryFileSystem = project
        .copy_to_memory_fs()
        .expect("Failed to copy project to memory fs");
    let system = TestSystem::new(InMemorySystem::from_memory_fs(fs));

    let src_root = SystemPath::new("/");
    let mut metadata = ProjectMetadata::discover(src_root, &system).expect("project discovery");

    metadata.apply_options(Options {
        environment: Some(EnvironmentOptions {
            python_version: Some(RangedValue::cli(project.config.python_version)),
            ..EnvironmentOptions::default()
        }),
        ..Options::default()
    });

    let mut db = ProjectDatabase::fallible(metadata, system).expect("db construction");

    db.project().set_included_paths(
        &mut db,
        project
            .check_paths()
            .iter()
            .map(|p| SystemPathBuf::from(*p))
            .collect(),
    );

    db
}

/// Walk every file in the project's source paths, run `document_symbols`,
/// classify each symbol, and group the results by category. Positions
/// where `prepare_call_hierarchy` returns `None` are dropped — those would
/// just be no-ops at bench time and skew toward measuring failure cost
/// rather than real work.
fn harvest_positions(db: &ProjectDatabase) -> std::collections::HashMap<Category, Vec<Position>> {
    use std::collections::HashMap;

    let mut by_category: HashMap<Category, Vec<(SystemPathBuf, Position)>> = HashMap::new();

    let files: Vec<File> = db.project().files(db).iter().copied().collect();
    for file in files {
        let Some(path) = file.path(db).as_system_path() else {
            continue;
        };
        let path_str = path.as_str();
        if !std::path::Path::new(path_str)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("py"))
        {
            continue;
        }
        // Restrict to project sources; skip site-packages, typeshed, etc.
        if !path_str.starts_with("/src/") && !path_str.starts_with("/test") {
            continue;
        }

        let symbols = ty_ide::document_symbols(db, file);
        for (_, info) in symbols.iter() {
            let Some(category) = Category::classify(&info.name, info.kind) else {
                continue;
            };
            let position = Position {
                file,
                offset: info.name_range.start(),
            };
            // Drop positions that ty doesn't recognise as call-hierarchy
            // targets — typically attributes / type aliases that
            // `document_symbols` reports but `prepare_call_hierarchy`
            // declines.
            if ty_ide::prepare_call_hierarchy(db, position.file, position.offset).is_none() {
                continue;
            }
            by_category
                .entry(category)
                .or_default()
                .push((path.to_owned(), position));
        }
    }

    // Deterministic order: sort by file path then offset, then take up to
    // MAX_PER_CATEGORY. Same anyio commit + same iteration order across
    // runs => same sampled positions.
    let mut out: HashMap<Category, Vec<Position>> = HashMap::new();
    for (category, mut positions) in by_category {
        positions.sort_by(|a, b| {
            a.0.as_str()
                .cmp(b.0.as_str())
                .then_with(|| a.1.offset.cmp(&b.1.offset))
        });
        let trimmed: Vec<Position> = positions
            .into_iter()
            .take(MAX_PER_CATEGORY)
            .map(|(_, p)| p)
            .collect();
        out.insert(category, trimmed);
    }
    out
}

/// Pre-run all three phases at every harvested position so salsa caches
/// (`parsed_module`, semantic index, type inference) are populated. Without
/// this each bench's first iter would be dominated by cold-cache work.
fn warm(fixture: &Fixture) {
    for positions in fixture.by_category.values() {
        for p in positions {
            let _ = ty_ide::prepare_call_hierarchy(&fixture.db, p.file, p.offset);
            let _ = ty_ide::incoming_calls(&fixture.db, p.file, p.offset);
            let _ = ty_ide::outgoing_calls(&fixture.db, p.file, p.offset);
        }
    }
}

fn build_fixture() -> Fixture {
    let project = Box::leak(Box::new(setup_anyio()));
    let db = build_db(project);
    let by_category = harvest_positions(&db);
    let fixture = Fixture { db, by_category };
    warm(&fixture);
    fixture
}

static RAYON_INITIALIZED: std::sync::Once = std::sync::Once::new();
fn setup_rayon() {
    // Single-threaded so we measure ty's per-call cost, not how well rayon
    // scales. The LSP bench owns concurrent measurement.
    RAYON_INITIALIZED.call_once(|| {
        ThreadPoolBuilder::new()
            .num_threads(1)
            .use_current_thread()
            .build_global()
            .unwrap();
    });
}

/// One sub-bench: time how long it takes to run `phase` at every position
/// in `positions`. The aggregate per-iter time divided by `positions.len()`
/// is the per-call cost; criterion reports the aggregate, which is the
/// stable quantity to compare across runs.
fn run_phase_bench(
    criterion: &mut Criterion,
    fixture: &Fixture,
    category: Category,
    phase: &str,
    f: fn(&ProjectDatabase, File, TextSize),
) {
    let positions = match fixture.by_category.get(&category) {
        Some(p) if !p.is_empty() => p,
        _ => return,
    };

    let mut group = criterion.benchmark_group("ty_ide_project");
    // Flat sampling keeps the per-bench output stable when iters are
    // long (one iter loops through up to MAX_PER_CATEGORY positions).
    group.sampling_mode(SamplingMode::Flat);
    let bench_name = format!("{}_{}[n={}]", category.as_str(), phase, positions.len());
    group.bench_function(&bench_name, |b| {
        b.iter_batched_ref(
            || (),
            |()| {
                for p in positions {
                    f(&fixture.db, p.file, p.offset);
                }
            },
            BatchSize::SmallInput,
        );
    });
    group.finish();
}

fn bench_call_hierarchy(criterion: &mut Criterion) {
    setup_rayon();
    let fixture = build_fixture();

    for category in [
        Category::Function,
        Category::Method,
        Category::Class,
        Category::Property,
        Category::Dunder,
    ] {
        run_phase_bench(criterion, &fixture, category, "prepare", |db, f, o| {
            let _ = ty_ide::prepare_call_hierarchy(db, f, o);
        });
        run_phase_bench(criterion, &fixture, category, "incoming", |db, f, o| {
            let _ = ty_ide::incoming_calls(db, f, o);
        });
        run_phase_bench(criterion, &fixture, category, "outgoing", |db, f, o| {
            let _ = ty_ide::outgoing_calls(db, f, o);
        });
    }
}

criterion_group!(call_hierarchy, bench_call_hierarchy);
criterion_main!(call_hierarchy);
