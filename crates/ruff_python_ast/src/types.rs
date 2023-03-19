use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::path::PathBuf;

use rustpython_parser::ast::{Expr, Located, Location, Stmt};

#[derive(Clone)]
pub enum Node<'a> {
    Stmt(&'a Stmt),
    Expr(&'a Expr),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Range {
    pub location: Location,
    pub end_location: Location,
}

impl Range {
    pub const fn new(location: Location, end_location: Location) -> Self {
        Self {
            location,
            end_location,
        }
    }
}

impl<T> From<&Located<T>> for Range {
    fn from(located: &Located<T>) -> Self {
        Range::new(located.location, located.end_location.unwrap())
    }
}

impl<T> From<&Box<Located<T>>> for Range {
    fn from(located: &Box<Located<T>>) -> Self {
        Range::new(located.location, located.end_location.unwrap())
    }
}

#[derive(Debug, Copy, Clone)]
pub struct RefEquality<'a, T>(pub &'a T);

impl<'a, T> std::hash::Hash for RefEquality<'a, T> {
    fn hash<H>(&self, state: &mut H)
    where
        H: std::hash::Hasher,
    {
        (self.0 as *const T).hash(state);
    }
}

impl<'a, 'b, T> PartialEq<RefEquality<'b, T>> for RefEquality<'a, T> {
    fn eq(&self, other: &RefEquality<'b, T>) -> bool {
        std::ptr::eq(self.0, other.0)
    }
}

impl<'a, T> Eq for RefEquality<'a, T> {}

impl<'a, T> Deref for RefEquality<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.0
    }
}

impl<'a> From<&RefEquality<'a, Stmt>> for &'a Stmt {
    fn from(r: &RefEquality<'a, Stmt>) -> Self {
        r.0
    }
}

impl<'a> From<&RefEquality<'a, Expr>> for &'a Expr {
    fn from(r: &RefEquality<'a, Expr>) -> Self {
        r.0
    }
}

pub type CallPath<'a> = smallvec::SmallVec<[&'a str; 8]>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    pub name: String,
    pub location: Location,
    pub end_location: Location,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Imports {
    inner: FxHashMap<PathBuf, Vec<Import>>,
}

impl Imports {
    pub fn insert(&mut self, module: PathBuf, imports_vec: Vec<Import>) {
        self.inner.insert(module, imports_vec);
    }

    pub fn extend(&mut self, other: Self) {
        self.inner.extend(other.inner);
    }

    pub fn expand_relative(
        modules: &[&str],
        module: &Option<String>,
        name: &str,
        level: &Option<usize>,
    ) -> String {
        match level {
            Some(level) => {
                // last part of path is always the module itself
                let resolved_module = if *level > 0 {
                    format!(
                        "{}.",
                        modules
                            .iter()
                            .rev()
                            .take(*level)
                            .collect::<Vec<_>>()
                            .iter()
                            .rev()
                            .map(|s| (**s).to_string())
                            .collect::<Vec<String>>()
                            .join(".")
                    )
                } else {
                    String::new()
                };
                if let Some(module_name) = module.as_ref() {
                    format!("{resolved_module}{module_name}.{name}")
                } else {
                    format!("{resolved_module}{name}")
                }
            }
            None => format!("{}.{}", module.as_ref().unwrap_or(&String::new()), name),
        }
    }
}
