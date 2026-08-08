use std::process::Command;

use divan::{Bencher, bench};
use rayon::ThreadPoolBuilder;
use ruff_db::system::{OsSystem, SystemPath, TestSystem};
use ty_project::{ProjectDatabase, ProjectMetadata};

fn setup_iteration(root: &SystemPath) -> ProjectDatabase {
    let system = TestSystem::new(OsSystem::new(root));
    system.set_env_var("TY_UV", "scripts");

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

from attrs import define


@define
class User:
    name: str


def greet(user: User) -> str:
    return f"Hello, {user.name}!"
"#,
    )
    .unwrap();

    let uv = std::env::var_os("UV").unwrap_or_else(|| "uv".into());
    let output = Command::new(uv)
        .args(["workspace", "metadata", "--sync", "--script"])
        .arg(root.join("script.py").as_std_path())
        .current_dir(root.as_std_path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "failed to prepare script environment: {}",
        String::from_utf8_lossy(&output.stderr)
    );

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
