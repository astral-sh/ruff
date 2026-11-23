//! Benchmark-only prototype for reusing the formatter's file discovery.
use std::fs;
use std::hash::Hasher;
use std::io::{self, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rayon::prelude::*;
use ruff_cache::{CacheKey, CacheKeyHasher};
use ruff_workspace::resolver::{
    ConfigurationTransformer, PyprojectDiscoveryStrategy, ResolvedFile, ResolvedFiles, Resolver,
    RootProject, project_files_in_path, project_files_in_path_with_directories,
};
use serde::{Deserialize, Serialize};
use tracing::debug;

pub(super) fn project_files<'a>(
    paths: &[PathBuf],
    root_project: &'a RootProject,
    transformer: &(dyn ConfigurationTransformer + Sync),
    can_cache: bool,
) -> Result<(ResolvedFiles, Resolver<'a>)> {
    let settings = &root_project.project.settings;
    let Some(path) = paths.first().filter(|_| {
        can_cache
            && paths.len() == 1
            && std::env::var_os("RUFF_EXPERIMENTAL_FORMAT_DISCOVERY_CACHE")
                .is_some_and(|v| v == "1")
            && matches!(root_project.strategy, PyprojectDiscoveryStrategy::Fixed)
            && !settings.file_resolver.respect_gitignore
    }) else {
        return project_files_in_path(paths, root_project, transformer);
    };

    // Only use this for one real directory. Explicit files, symlinked roots and overlapping
    // input paths have different discovery semantics.
    let root = ruff_linter::fs::normalize_path(path);
    if !root.is_dir() || root.canonicalize().ok().as_deref() != Some(root.as_path()) {
        return project_files_in_path(paths, root_project, transformer);
    }
    let mut key = CacheKeyHasher::new();
    root.cache_key(&mut key);
    settings.file_resolver.cache_key(&mut key);
    let cache_dir = settings.cache_dir.join(ruff_linter::VERSION);
    let cache_path = cache_dir.join(format!("format-discovery-v1-{}", key.finish()));

    if let Some(snapshot) = fs::read(&cache_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Snapshot>(&bytes).ok())
        .filter(|snapshot| snapshot.root == root && snapshot.is_current())
    {
        debug!("Format discovery cache hit");
        let files = snapshot
            .files
            .into_iter()
            .map(|path| Ok(ResolvedFile::Nested(path)))
            .collect();
        return Ok((files, Resolver::new(root_project)));
    }
    debug!("Format discovery cache miss");

    let started = SystemTime::now();
    let directories = Mutex::new(Some(Vec::new()));
    let visit = |path: &Path| {
        let stamp = DirectoryStamp::read(path).ok().filter(|stamp| {
            // The walker opens the directory just before calling the visitor. Avoid persisting
            // a snapshot if a directory could have changed between those two operations.
            stamp.predates(started)
        });
        let Ok(mut dirs) = directories.lock() else {
            return;
        };
        if let Some(entries) = dirs.as_mut()
            && let Some(stamp) = stamp
        {
            entries.push((path.to_path_buf(), stamp));
        } else {
            *dirs = None;
        }
    };
    let (files, resolver) =
        project_files_in_path_with_directories(paths, root_project, transformer, Some(&visit))?;
    if let Ok(Some(directories)) = directories.into_inner()
        && directories.iter().any(|(path, _)| *path == root)
        && let Some(nested_files) = files
            .iter()
            .map(|file| match file {
                Ok(ResolvedFile::Nested(path)) => Some(path.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
    {
        let snapshot = Snapshot {
            root,
            directories,
            files: nested_files,
        };
        if snapshot.is_current() {
            let _ = snapshot.store(&cache_dir, &cache_path);
        }
    }
    Ok((files, resolver))
}

#[derive(Serialize, Deserialize)]
struct Snapshot {
    root: PathBuf,
    directories: Vec<(PathBuf, DirectoryStamp)>,
    files: Vec<PathBuf>,
}

impl Snapshot {
    fn is_current(&self) -> bool {
        self.directories
            .par_iter()
            .all(|(path, stamp)| DirectoryStamp::read(path).is_ok_and(|current| current == *stamp))
    }

    fn store(&self, parent: &Path, path: &Path) -> Result<()> {
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec(self)?)?;
        temp.persist(path)?;
        Ok(())
    }
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct DirectoryStamp {
    device: u64,
    inode: u64,
    mode: u32,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

impl DirectoryStamp {
    fn read(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_dir() {
            return Err(io::ErrorKind::NotADirectory.into());
        }
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            mode: metadata.mode(),
            mtime: (metadata.mtime(), metadata.mtime_nsec()),
            ctime: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }

    fn predates(&self, started: SystemTime) -> bool {
        let Ok(before) = started.duration_since(UNIX_EPOCH) else {
            return false;
        };
        [self.ctime, self.mtime]
            .into_iter()
            .all(|(seconds, nanos)| {
                u64::try_from(seconds)
                    .ok()
                    .zip(u32::try_from(nanos).ok())
                    .is_some_and(|(seconds, nanos)| {
                        Duration::new(seconds, nanos) + Duration::from_secs(2) < before
                    })
            })
    }
}
