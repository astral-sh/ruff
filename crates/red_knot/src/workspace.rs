use std::collections::BTreeMap;

use ruff_db::system::{SystemPath, SystemPathBuf};

use crate::program::Program;

pub struct Workspace {
    /// The root path of the workspace.
    ///
    /// This is the path to the common ancestor directory of all programs containing a ruff configuration file
    /// or the current working directory if the programs have no common ancestor directory (e.g. `C:\foo` and `D:\bar`)
    /// or no configuration file. This is the same as [`Program::path`] if there is only one program in the workspace.
    path: SystemPathBuf,

    /// The programs that are part of this workspace.
    ///
    /// The key is the directory containing the program.
    programs: BTreeMap<SystemPathBuf, Program>,
}

impl Workspace {
    pub fn new(path: impl AsRef<SystemPath>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            programs: BTreeMap::default(),
        }
    }

    pub fn add_program(&mut self, program: Program) {
        self.programs.insert(program.path().to_path_buf(), program);
    }

    pub fn path(&self) -> &SystemPath {
        &self.path
    }

    /// Returns the closest program that contains the given path.
    pub fn program(&self, path: impl AsRef<SystemPath>) -> Option<&Program> {
        let path = path.as_ref();
        for (program_path, program) in self.programs.range(..=path.to_path_buf()).rev() {
            if path.starts_with(program_path) {
                return Some(program);
            }
        }

        None
    }

    pub fn program_mut(&mut self, path: impl AsRef<SystemPath>) -> Option<&mut Program> {
        let path = path.as_ref();
        for (program_path, program) in self.programs.range_mut(..=path.to_path_buf()).rev() {
            if path.starts_with(program_path) {
                return Some(program);
            }
        }

        None
    }
}
