//! Detect Python package roots and file associations.

use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::resolver::{PyprojectDiscovery, Resolver};

// If we have a Python package layout like:
// - root/
//   - foo/
//     - __init__.py
//     - bar.py
//     - baz/
//       - __init__.py
//       - qux.py
//
// Then today, if you run with defaults (`src = ["."]`) from `root`, we'll
// detect that `foo.bar`, `foo.baz`, and `foo.baz.qux` are first-party modules
// (since, if you're in `root`, you can see `foo`).
//
// However, we'd also like it to be the case that, even if you run this command
// from `foo`, we still consider `foo.baz.qux` to be first-party when linting
// `foo/bar.py`. More specifically, for each Python file, we should find the
// root of the current package.
//
// Thus, for each file, we iterate up its ancestors, returning the last
// directory containing an `__init__.py`.

/// Return `true` if the directory at the given `Path` appears to be a Python
/// package.
pub fn is_package(path: &Path, namespace_packages: &[PathBuf]) -> bool {
    path.join("__init__.py").is_file()
        || namespace_packages
            .iter()
            .any(|namespace_package| namespace_package == path)
}

/// Return the package root for the given Python file.
pub fn detect_package_root<'a>(
    path: &'a Path,
    namespace_packages: &'a [PathBuf],
) -> Option<&'a Path> {
    let mut current = None;
    for parent in path.ancestors() {
        if !is_package(parent, namespace_packages) {
            return current;
        }
        current = Some(parent);
    }
    current
}

/// A wrapper around `is_package` to cache filesystem lookups.
fn is_package_with_cache<'a>(
    path: &'a Path,
    namespace_packages: &'a [PathBuf],
    package_cache: &mut FxHashMap<&'a Path, bool>,
) -> bool {
    *package_cache
        .entry(path)
        .or_insert_with(|| is_package(path, namespace_packages))
}

/// A wrapper around `detect_package_root` to cache filesystem lookups.
fn detect_package_root_with_cache<'a>(
    path: &'a Path,
    namespace_packages: &'a [PathBuf],
    package_cache: &mut FxHashMap<&'a Path, bool>,
) -> Option<&'a Path> {
    let mut current = None;
    for parent in path.ancestors() {
        if !is_package_with_cache(parent, namespace_packages, package_cache) {
            return current;
        }
        current = Some(parent);
    }
    current
}

/// Return a mapping from Python file to its package root.
pub fn detect_package_roots<'a>(
    files: &[&'a Path],
    resolver: &'a Resolver,
    pyproject_strategy: &'a PyprojectDiscovery,
) -> FxHashMap<&'a Path, Option<&'a Path>> {
    // Pre-populate the module cache, since the list of files could (but isn't
    // required to) contain some `__init__.py` files.
    let mut package_cache: FxHashMap<&Path, bool> = FxHashMap::default();
    for file in files {
        if file.ends_with("__init__.py") {
            if let Some(parent) = file.parent() {
                package_cache.insert(parent, true);
            }
        }
    }

    // Search for the package root for each file.
    let mut package_roots: FxHashMap<&Path, Option<&Path>> = FxHashMap::default();
    for file in files {
        let namespace_packages = &resolver
            .resolve(file, pyproject_strategy)
            .namespace_packages;
        if let Some(package) = file.parent() {
            if package_roots.contains_key(package) {
                continue;
            }
            package_roots.insert(
                package,
                detect_package_root_with_cache(package, namespace_packages, &mut package_cache),
            );
        }
    }

    package_roots
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::packaging::detect_package_root;
    use crate::test::test_resource_path;

    #[test]
    fn package_detection() {
        assert_eq!(
            detect_package_root(&test_resource_path("package/src/package"), &[],),
            Some(test_resource_path("package/src/package").as_path())
        );

        assert_eq!(
            detect_package_root(&test_resource_path("project/python_modules/core/core"), &[],),
            Some(test_resource_path("project/python_modules/core/core").as_path())
        );

        assert_eq!(
            detect_package_root(
                &test_resource_path("project/examples/docs/docs/concepts"),
                &[],
            ),
            Some(test_resource_path("project/examples/docs/docs").as_path())
        );

        assert_eq!(
            detect_package_root(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("setup.py")
                    .as_path(),
                &[],
            ),
            None,
        );
    }
}
