use std::fmt;

use ruff_db::files::File;
use ruff_python_ast::PythonVersion;

use crate::{Db, ModuleResolveMode, SearchPaths, search_paths};

/// The Python version and search paths used to resolve modules.
#[salsa::interned(debug, heap_size = ruff_memory_usage::heap_size)]
pub struct ResolverEnvironment<'db> {
    #[returns(copy)]
    pub python_version: PythonVersion,

    #[returns(ref)]
    pub search_paths: SearchPaths,
}

impl get_size2::GetSize for ResolverEnvironment<'_> {}

impl<'db> ResolverEnvironment<'db> {
    pub fn display_search_paths(
        self,
        db: &'db dyn Db,
        mode: ModuleResolveMode,
    ) -> DisplaySearchPaths<'db> {
        DisplaySearchPaths {
            db,
            resolver_environment: self,
            mode,
        }
    }
}

pub struct DisplaySearchPaths<'db> {
    db: &'db dyn Db,
    resolver_environment: ResolverEnvironment<'db>,
    mode: ModuleResolveMode,
}

impl fmt::Display for DisplaySearchPaths<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut paths = search_paths(self.db, self.resolver_environment, self.mode).peekable();

        if paths.peek().is_none() {
            return f.write_str("[]");
        }

        writeln!(f, "[")?;
        for path in paths {
            writeln!(f, "  {path},")?;
        }
        f.write_str("]")
    }
}

/// A physical file interpreted in one module-resolution environment.
#[salsa::interned(debug, revisions = usize::MAX, heap_size = ruff_memory_usage::heap_size)]
pub struct ResolverFile<'db> {
    #[returns(copy)]
    pub file: File,

    #[returns(copy)]
    pub environment: ResolverEnvironment<'db>,
}

impl get_size2::GetSize for ResolverFile<'_> {}
