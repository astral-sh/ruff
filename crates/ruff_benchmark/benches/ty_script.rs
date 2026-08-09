use divan::{Bencher, bench};
use rayon::ThreadPoolBuilder;
use ruff_db::system::{OsSystem, SystemPath, TestSystem};
use ty_project::{ProjectDatabase, ProjectMetadata};

fn setup_iteration(root: &SystemPath) -> ProjectDatabase {
    let system = TestSystem::new(OsSystem::new(root));

    let metadata = ProjectMetadata::new("script", root.to_path_buf());
    ProjectDatabase::fallible(metadata, system).unwrap()
}

#[bench(sample_size = 2, sample_count = 3)]
fn simple_script(bencher: Bencher) {
    let directory = tempfile::tempdir().unwrap();
    let root = SystemPath::from_std_path(directory.path()).unwrap();
    std::fs::write(
        root.join("script.py"),
        r#"# /// script
# requires-python = ">=3.12"
# dependencies = ["attrs==25.4.0"]
# ///

class User:
    name: str


def greet(user: User) -> str:
    return f"Hello, {user.name}!"
"#,
    )
    .unwrap();

    bencher
        .with_inputs(|| setup_iteration(root))
        .bench_local_refs(|db| assert!(db.check().is_empty()));
}

fn main() {
    ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .unwrap();

    divan::main();
}
