use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};

use globset::{GlobMatcher, GlobSet};
use itertools::Itertools;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};

use super::types::FilePattern;

#[derive(Debug)]
pub struct HashableRegex(Regex);

impl Hash for HashableRegex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_str().hash(state);
    }
}

impl From<Regex> for HashableRegex {
    fn from(regex: Regex) -> Self {
        Self(regex)
    }
}

impl Deref for HashableRegex {
    type Target = Regex;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug)]
pub struct HashableGlobMatcher(GlobMatcher);

impl From<GlobMatcher> for HashableGlobMatcher {
    fn from(matcher: GlobMatcher) -> Self {
        Self(matcher)
    }
}

impl Deref for HashableGlobMatcher {
    type Target = GlobMatcher;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Hash for HashableGlobMatcher {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.glob().hash(state);
    }
}

#[derive(Debug)]
pub struct HashableGlobSet {
    patterns: Vec<FilePattern>,
    globset: GlobSet,
}

impl HashableGlobSet {
    pub fn new(patterns: Vec<FilePattern>) -> anyhow::Result<Self> {
        let mut builder = globset::GlobSetBuilder::new();
        for pattern in &patterns {
            pattern.clone().add_to(&mut builder)?;
        }
        let globset = builder.build()?;
        Ok(HashableGlobSet { patterns, globset })
    }

    pub fn empty() -> Self {
        Self {
            patterns: Vec::new(),
            globset: GlobSet::empty(),
        }
    }
}

impl Deref for HashableGlobSet {
    type Target = GlobSet;

    fn deref(&self) -> &Self::Target {
        &self.globset
    }
}

impl Hash for HashableGlobSet {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for pattern in self.patterns.iter().sorted() {
            pattern.hash(state);
        }
    }
}

#[derive(Debug, Clone)]
pub struct HashableHashSet<T>(FxHashSet<T>);

impl<T: Hash + Ord> Hash for HashableHashSet<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for v in self.0.iter().sorted() {
            v.hash(state);
        }
    }
}

impl<T> Default for HashableHashSet<T> {
    fn default() -> Self {
        Self(FxHashSet::default())
    }
}

impl<T> From<FxHashSet<T>> for HashableHashSet<T> {
    fn from(set: FxHashSet<T>) -> Self {
        Self(set)
    }
}

impl<T> From<HashableHashSet<T>> for FxHashSet<T> {
    fn from(set: HashableHashSet<T>) -> Self {
        set.0
    }
}

impl<T> Deref for HashableHashSet<T> {
    type Target = FxHashSet<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct HashableHashMap<K, V>(FxHashMap<K, V>);

impl<K: Hash + Ord, V: Hash> Hash for HashableHashMap<K, V> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for key in self.0.keys().sorted() {
            key.hash(state);
            self.0[key].hash(state);
        }
    }
}

impl<K, V> Default for HashableHashMap<K, V> {
    fn default() -> Self {
        Self(FxHashMap::default())
    }
}

impl<K, V> From<FxHashMap<K, V>> for HashableHashMap<K, V> {
    fn from(map: FxHashMap<K, V>) -> Self {
        Self(map)
    }
}

impl<K, V> From<HashableHashMap<K, V>> for FxHashMap<K, V> {
    fn from(map: HashableHashMap<K, V>) -> Self {
        map.0
    }
}

impl<K, V> Deref for HashableHashMap<K, V> {
    type Target = FxHashMap<K, V>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> DerefMut for HashableHashMap<K, V> {
    fn deref_mut(&mut self) -> &mut <Self as Deref>::Target {
        &mut self.0
    }
}
