use std::process::Command;

use divan::{Bencher, bench};
use rayon::ThreadPoolBuilder;
use ruff_db::system::{OsSystem, SystemPath, TestSystem};
use ty_project::{ProjectDatabase, ProjectMetadata};
use ty_static::EnvVars;

fn setup_iteration(root: &SystemPath) -> ProjectDatabase {
    let system = TestSystem::new(OsSystem::new(root));
    for name in [
        EnvVars::VIRTUAL_ENV,
        EnvVars::CONDA_PREFIX,
        EnvVars::CONDA_DEFAULT_ENV,
        EnvVars::CONDA_ROOT,
        EnvVars::PYTHONPATH,
    ] {
        system.remove_env_var(name);
    }
    system.set_env_var(EnvVars::TY_UV, "scripts");
    system.set_env_var(EnvVars::UV, "uv");

    let metadata = ProjectMetadata::discover(root, &system).unwrap();
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

    // Synchronize once before measuring so this benchmark covers the warm uv cache case.
    let output = Command::new("uv")
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
