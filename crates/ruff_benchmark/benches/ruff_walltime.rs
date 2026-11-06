use std::ffi::OsString;
use std::fmt;

use anyhow::{Result, ensure};
use clap::Parser;
use divan::Bencher;
use rayon::ThreadPoolBuilder;
use ruff::ExitStatus;
use ruff::args::Args;
use ruff_benchmark::checkout::checkout_project;

#[cfg(target_os = "windows")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    not(target_os = "windows"),
    not(target_os = "openbsd"),
    not(target_os = "aix"),
    not(target_os = "android"),
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "powerpc64",
        target_arch = "riscv64"
    )
))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

struct Project {
    name: &'static str,
    repository: &'static str,
    commit: &'static str,
    path: &'static str,
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.name.fmt(f)
    }
}

// A large tree of already-formatted Python packages. Most modules define
// async functions, and the project enables Ruff's `ASYNC` lint rules.
const HOME_ASSISTANT: Project = Project {
    name: "home-assistant",
    repository: "https://github.com/home-assistant/core",
    commit: "1647c0bf847609bf6da881a78ea0f166a9adcd13",
    path: "homeassistant/components",
};

// The Python files are ASCII-only and mostly unformatted (58 of 66 needed formatting on 2026-11-05).
const DOCKER_PY: Project = Project {
    name: "docker-py",
    repository: "https://github.com/docker/docker-py",
    commit: "e734fd5a5508bc1062a28a51dc2f31ae47af7a2f",
    path: "docker",
};

// Lints `.pyi` files with typeshed's selected `PYI` rules.
const TYPESHED: Project = Project {
    name: "typeshed",
    repository: "https://github.com/python/typeshed",
    commit: "e0efbeef901e9b6998d016e1ab9352678f09ae77",
    path: "stdlib",
};

#[divan::bench(args = [HOME_ASSISTANT, DOCKER_PY], sample_count = 8, sample_size = 1)]
fn format_uncached(bencher: Bencher, project: &Project) {
    benchmark(bencher, project, &["format", "--check"], false).unwrap();
}

#[divan::bench(args = [HOME_ASSISTANT, DOCKER_PY], sample_count = 8, sample_size = 1)]
fn format_cached(bencher: Bencher, project: &Project) {
    benchmark(bencher, project, &["format", "--check"], true).unwrap();
}

// typeshed sets `fix = true`. Both lint benchmarks pass `--no-fix` so the
// cache warm-up and every measured iteration see the original source text.
#[divan::bench(args = [HOME_ASSISTANT, TYPESHED], sample_count = 8, sample_size = 1)]
fn lint_uncached(bencher: Bencher, project: &Project) {
    benchmark(bencher, project, &["check", "--no-fix"], false).unwrap();
}

#[divan::bench(args = [HOME_ASSISTANT, TYPESHED], sample_count = 8, sample_size = 1)]
fn lint_cached(bencher: Bencher, project: &Project) {
    benchmark(bencher, project, &["check", "--no-fix"], true).unwrap();
}

fn benchmark(bencher: Bencher, project: &Project, args: &[&str], cached: bool) -> Result<()> {
    let checkout = checkout_project(project.name, project.repository, project.commit)?;
    let path = checkout.join(project.path);
    ensure!(path.is_dir(), "Missing benchmark input {}", path.display());

    let mut argv = vec![OsString::from("ruff"), "--silent".into()];
    argv.extend(args.iter().map(OsString::from));
    argv.push(path.into_os_string());
    let cache = cached.then(tempfile::tempdir).transpose()?;
    if let Some(cache) = &cache {
        argv.extend(["--cache-dir".into(), cache.path().as_os_str().to_owned()]);
    } else {
        argv.push("--no-cache".into());
    }

    let input = || Args::parse_from(&argv);
    if let Some(cache) = &cache {
        run_ruff(input());
        ensure!(
            cache.path().read_dir()?.next().is_some(),
            "Ruff did not prime the cache for {project}"
        );
    }
    bencher.with_inputs(input).bench_local_values(run_ruff);
    Ok(())
}

fn run_ruff(args: Args) {
    let status = ruff::run_command(args).expect("Ruff command failed");
    assert!(!matches!(status, ExitStatus::Error), "Ruff command failed");
}

fn main() {
    ThreadPoolBuilder::new()
        .num_threads(1)
        .use_current_thread()
        .build_global()
        .expect("Initialize benchmark worker pool");
    divan::main();
}
