use crate::combine::Combine;
use crate::Db;
use ruff_db::system::{System, SystemPath, SystemPathBuf};
use ruff_macros::Combine;
use ruff_text_size::{TextRange, TextSize};
use serde::{Deserialize, Deserializer};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use toml::Spanned;

#[derive(Clone, Debug)]
pub enum ValueSource {
    /// Value loaded from a project's configuration file.
    ///
    /// Ideally, we'd use [`ruff_db::files::File`] but we can't because the database hasn't been
    /// created when loading the configuration.
    File(Arc<SystemPathBuf>),

    /// The value comes from a CLI argument, while it's left open if specified using a short argument,
    /// long argument (`--extra-paths`) or `--config key=value`.
    Cli,
}

impl ValueSource {
    pub fn file(&self) -> Option<&SystemPath> {
        match self {
            ValueSource::File(path) => Some(&**path),
            ValueSource::Cli => None,
        }
    }
}

thread_local! {
    /// Serde doesn't provide any easy means to pass a value to a [`Deserialize`] implementation,
    /// but we want to associate each deserialized [`RelativePath`] with the source from
    /// which it originated. We use a thread local variable to work around this limitation.
    ///
    /// Use the [`ValueSourceGuard`] to initialize the thread local before calling into any
    /// deserialization code. It ensures that the thread local variable gets cleaned up
    /// once deserialization is done (once the guard gets dropped).
    static VALUE_SOURCE: RefCell<Option<(ValueSource, bool)>> = const { RefCell::new(None) };
}

/// Guard to safely change the [`VALUE_SOURCE`] for the current thread.
#[must_use]
pub(super) struct ValueSourceGuard {
    prev_value: Option<(ValueSource, bool)>,
}

impl ValueSourceGuard {
    pub(super) fn new(source: ValueSource, is_toml: bool) -> Self {
        let prev = VALUE_SOURCE.replace(Some((source, is_toml)));
        Self { prev_value: prev }
    }
}

impl Drop for ValueSourceGuard {
    fn drop(&mut self) {
        VALUE_SOURCE.set(self.prev_value.take());
    }
}

/// A value that "remembers" where it comes from (source) and its range in source.
///
/// ## Equality, Hash, and Ordering
/// The equality, hash, and ordering are solely based on the value. They disregard the value's range
/// or source.
///
/// This ensures that two resolved configurations are identical even if the position of a value has changed
/// or if the values were loaded from different sources.
#[derive(Clone, serde::Serialize)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RangedValue<T> {
    value: T,
    #[serde(skip)]
    source: ValueSource,

    /// The byte range of `value` in `source`.
    ///
    /// Can be `None` because not all sources support a range.
    /// For example, arguments provided on the CLI won't have a range attached.
    #[serde(skip)]
    range: Option<TextRange>,
}

impl<T> RangedValue<T> {
    pub fn new(value: T, source: ValueSource) -> Self {
        Self::with_range(value, source, TextRange::default())
    }

    pub fn cli(value: T) -> Self {
        Self::with_range(value, ValueSource::Cli, TextRange::default())
    }

    pub fn with_range(value: T, source: ValueSource, range: TextRange) -> Self {
        Self {
            value,
            range: Some(range),
            source,
        }
    }

    pub fn range(&self) -> Option<TextRange> {
        self.range
    }

    pub fn source(&self) -> &ValueSource {
        &self.source
    }

    #[must_use]
    pub fn with_source(mut self, source: ValueSource) -> Self {
        self.source = source;
        self
    }

    #[must_use]
    pub fn map_value<R>(self, f: impl FnOnce(T) -> R) -> RangedValue<R> {
        RangedValue {
            value: f(self.value),
            source: self.source,
            range: self.range,
        }
    }

    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Combine for RangedValue<T> {
    fn combine(self, _other: Self) -> Self
    where
        Self: Sized,
    {
        self
    }
    fn combine_with(&mut self, _other: Self) {}
}

impl<T> IntoIterator for RangedValue<T>
where
    T: IntoIterator,
{
    type Item = T::Item;
    type IntoIter = T::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

// The type already has an `iter` method thanks to `Deref`.
#[expect(clippy::into_iter_without_iter)]
impl<'a, T> IntoIterator for &'a RangedValue<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

// The type already has a `into_iter_mut` method thanks to `DerefMut`.
#[expect(clippy::into_iter_without_iter)]
impl<'a, T> IntoIterator for &'a mut RangedValue<T>
where
    &'a mut T: IntoIterator,
{
    type Item = <&'a mut T as IntoIterator>::Item;
    type IntoIter = <&'a mut T as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.value.into_iter()
    }
}

impl<T> fmt::Debug for RangedValue<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> fmt::Display for RangedValue<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

impl<T> Deref for RangedValue<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for RangedValue<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<T, U: ?Sized> AsRef<U> for RangedValue<T>
where
    T: AsRef<U>,
{
    fn as_ref(&self) -> &U {
        self.value.as_ref()
    }
}

impl<T: PartialEq> PartialEq for RangedValue<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }
}

impl<T: PartialEq<T>> PartialEq<T> for RangedValue<T> {
    fn eq(&self, other: &T) -> bool {
        self.value.eq(other)
    }
}

impl<T: Eq> Eq for RangedValue<T> {}

impl<T: Hash> Hash for RangedValue<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T: PartialOrd> PartialOrd for RangedValue<T> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.value.partial_cmp(&other.value)
    }
}

impl<T: PartialOrd<T>> PartialOrd<T> for RangedValue<T> {
    fn partial_cmp(&self, other: &T) -> Option<Ordering> {
        self.value.partial_cmp(other)
    }
}

impl<T: Ord> Ord for RangedValue<T> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.value.cmp(&other.value)
    }
}

impl<'de, T> Deserialize<'de> for RangedValue<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        VALUE_SOURCE.with_borrow(|source| {
            let (source, has_span) = source.clone().unwrap();

            if has_span {
                let spanned: Spanned<T> = Spanned::deserialize(deserializer)?;
                let span = spanned.span();
                let range = TextRange::new(
                    TextSize::try_from(span.start)
                        .expect("Configuration file to be smaller than 4GB"),
                    TextSize::try_from(span.end)
                        .expect("Configuration file to be smaller than 4GB"),
                );

                Ok(Self::with_range(spanned.into_inner(), source, range))
            } else {
                Ok(Self::new(T::deserialize(deserializer)?, source))
            }
        })
    }
}

/// A possibly relative path in a configuration file.
///
/// Relative paths in configuration files or from CLI options
/// require different anchoring:
///
/// * CLI: The path is relative to the current working directory
/// * Configuration file: The path is relative to the project's root.
#[derive(
    Debug,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Combine,
)]
#[serde(transparent)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RelativePathBuf(RangedValue<SystemPathBuf>);

impl RelativePathBuf {
    pub fn new(path: impl AsRef<SystemPath>, source: ValueSource) -> Self {
        Self(RangedValue::new(path.as_ref().to_path_buf(), source))
    }

    pub fn cli(path: impl AsRef<SystemPath>) -> Self {
        Self::new(path, ValueSource::Cli)
    }

    /// Returns the relative path as specified by the user.
    pub fn path(&self) -> &SystemPath {
        &self.0
    }

    /// Returns the owned relative path.
    pub fn into_path_buf(self) -> SystemPathBuf {
        self.0.into_inner()
    }

    /// Resolves the absolute path for `self` based on its origin.
    pub fn absolute_with_db(&self, db: &dyn Db) -> SystemPathBuf {
        self.absolute(db.project().root(db), db.system())
    }

    /// Resolves the absolute path for `self` based on its origin.
    pub fn absolute(&self, project_root: &SystemPath, system: &dyn System) -> SystemPathBuf {
        let relative_to = match &self.0.source {
            ValueSource::File(_) => project_root,
            ValueSource::Cli => system.current_directory(),
        };

        SystemPath::absolute(&self.0, relative_to)
    }
}
