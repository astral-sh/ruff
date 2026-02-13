/*!
Provides a trait implementation for `which::Sys` based on `System`.

This lets us use the `which` crate to discover executables in `PATH`
in a way that doesn't break out of our `System` abstraction.
*/

use std::{
    env::VarError,
    ffi::{OsStr, OsString},
    io,
    path::{Path, PathBuf},
};

use which::sys::{Sys, SysMetadata, SysReadDirEntry};

use super::{DirectoryEntry, Metadata, System, SystemPath};

impl Sys for &'_ dyn System {
    type ReadDirEntry = DirectoryEntry;

    type Metadata = Metadata;

    fn is_windows(&self) -> bool {
        cfg!(windows)
    }

    fn current_dir(&self) -> io::Result<PathBuf> {
        Ok(self.current_directory().as_std_path().to_owned())
    }

    fn home_dir(&self) -> Option<PathBuf> {
        #[cfg(windows)]
        const NAME: &str = "USERPROFILE";
        #[cfg(not(windows))]
        const NAME: &str = "HOME";
        env_var_os(*self, NAME).map(PathBuf::from)
    }

    fn env_split_paths(&self, paths: &OsStr) -> Vec<PathBuf> {
        std::env::split_paths(paths).collect()
    }

    fn env_path(&self) -> Option<OsString> {
        env_var_os(*self, "PATH")
    }

    fn env_path_ext(&self) -> Option<OsString> {
        env_var_os(*self, "PATHEXT")
    }

    fn metadata(&self, path: &Path) -> io::Result<Self::Metadata> {
        self.path_metadata(system_path_from_std_path(path)?)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<Self::Metadata> {
        // N.B. Our `System` abstraction doesn't seem to know about
        // symlinks, so it isn't really possible to implement
        // symlink-only metadata here.
        //
        // Thankfully, the `which` crate only uses this in one place
        // as of 2026-02-10. It's used to support reparse points on
        // Windows. I think this is somewhat obscure, so we mush ahead
        // without it. We can reconsider how we implement this if this
        // becomes a problem. ---AG
        self.metadata(path)
    }

    fn read_dir(
        &self,
        path: &Path,
    ) -> io::Result<Box<dyn Iterator<Item = io::Result<Self::ReadDirEntry>>>> {
        let iter = self
            .read_directory(system_path_from_std_path(path)?)?
            .collect::<Vec<_>>()
            .into_iter();
        Ok(Box::new(iter))
    }

    fn is_valid_executable(&self, path: &Path) -> io::Result<bool> {
        Ok(self.is_executable(system_path_from_std_path(path)?))
    }
}

impl SysReadDirEntry for DirectoryEntry {
    fn file_name(&self) -> OsString {
        // DirectoryEntry should always have a file name
        self.path.file_name().unwrap().into()
    }

    fn path(&self) -> PathBuf {
        self.path.clone().into_std_path_buf()
    }
}

impl SysMetadata for Metadata {
    fn is_symlink(&self) -> bool {
        self.file_type.is_symlink()
    }

    fn is_file(&self) -> bool {
        self.file_type.is_file()
    }
}

fn env_var_os(system: &dyn System, name: &str) -> Option<OsString> {
    system.env_var(name).map_or_else(
        |e| match e {
            VarError::NotPresent => None,
            VarError::NotUnicode(path) => Some(path),
        },
        |x| Some(x.into()),
    )
}

fn system_path_from_std_path(path: &Path) -> io::Result<&SystemPath> {
    SystemPath::from_std_path(path).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidFilename,
            format!("invalid UTF-8: {}", path.display()),
        )
    })
}
