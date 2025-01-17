use std::borrow::Borrow;
use std::fmt::Formatter;
use std::ops::Deref;
use std::path;

use camino::{Utf8Components, Utf8Path, Utf8PathBuf};

#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Hash)]
pub struct VendoredPath(Utf8Path);

impl VendoredPath {
    pub fn new(path: &(impl AsRef<Utf8Path> + ?Sized)) -> &Self {
        let path = path.as_ref();
        // SAFETY: VendoredPath is marked as #[repr(transparent)] so the conversion from a
        // *const Utf8Path to a *const VendoredPath is valid.
        unsafe { &*(path as *const Utf8Path as *const VendoredPath) }
    }

    pub fn to_path_buf(&self) -> VendoredPathBuf {
        VendoredPathBuf(self.0.to_path_buf())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn as_utf8_path(&self) -> &camino::Utf8Path {
        &self.0
    }

    pub fn as_std_path(&self) -> &path::Path {
        self.0.as_std_path()
    }

    pub fn components(&self) -> Utf8Components {
        self.0.components()
    }

    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.0.extension()
    }

    #[must_use]
    pub fn with_pyi_extension(&self) -> VendoredPathBuf {
        VendoredPathBuf(self.0.with_extension("pyi"))
    }

    #[must_use]
    pub fn join(&self, other: impl AsRef<VendoredPath>) -> VendoredPathBuf {
        VendoredPathBuf(self.0.join(other.as_ref()))
    }

    #[must_use]
    pub fn ends_with(&self, suffix: impl AsRef<VendoredPath>) -> bool {
        self.0.ends_with(suffix.as_ref())
    }

    #[must_use]
    pub fn parent(&self) -> Option<&Self> {
        self.0.parent().map(Self::new)
    }

    #[must_use]
    pub fn file_stem(&self) -> Option<&str> {
        self.0.file_stem()
    }

    pub fn strip_prefix(
        &self,
        prefix: impl AsRef<VendoredPath>,
    ) -> Result<&Self, path::StripPrefixError> {
        self.0.strip_prefix(prefix.as_ref()).map(Self::new)
    }
}

impl ToOwned for VendoredPath {
    type Owned = VendoredPathBuf;

    fn to_owned(&self) -> VendoredPathBuf {
        self.to_path_buf()
    }
}

#[repr(transparent)]
#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub struct VendoredPathBuf(Utf8PathBuf);

impl Default for VendoredPathBuf {
    fn default() -> Self {
        Self::new()
    }
}

impl VendoredPathBuf {
    pub fn new() -> Self {
        Self(Utf8PathBuf::new())
    }

    #[inline]
    pub fn as_path(&self) -> &VendoredPath {
        VendoredPath::new(&self.0)
    }

    pub fn push(&mut self, component: impl AsRef<VendoredPath>) {
        self.0.push(component.as_ref())
    }
}

impl Borrow<VendoredPath> for VendoredPathBuf {
    fn borrow(&self) -> &VendoredPath {
        self.as_path()
    }
}

impl AsRef<VendoredPath> for VendoredPathBuf {
    fn as_ref(&self) -> &VendoredPath {
        self.as_path()
    }
}

impl AsRef<VendoredPath> for VendoredPath {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        self
    }
}

impl AsRef<VendoredPath> for Utf8Path {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        VendoredPath::new(self)
    }
}

impl AsRef<VendoredPath> for Utf8PathBuf {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        VendoredPath::new(self.as_path())
    }
}

impl AsRef<VendoredPath> for str {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        VendoredPath::new(self)
    }
}

impl AsRef<VendoredPath> for String {
    #[inline]
    fn as_ref(&self) -> &VendoredPath {
        VendoredPath::new(self)
    }
}

impl AsRef<path::Path> for VendoredPath {
    #[inline]
    fn as_ref(&self) -> &path::Path {
        self.0.as_std_path()
    }
}

impl AsRef<Utf8Path> for VendoredPath {
    #[inline]
    fn as_ref(&self) -> &Utf8Path {
        &self.0
    }
}

impl Deref for VendoredPathBuf {
    type Target = VendoredPath;

    fn deref(&self) -> &Self::Target {
        self.as_path()
    }
}

impl From<&str> for VendoredPathBuf {
    fn from(value: &str) -> Self {
        Self(Utf8PathBuf::from(value))
    }
}

impl<'a> TryFrom<&'a path::Path> for &'a VendoredPath {
    type Error = camino::FromPathError;

    fn try_from(value: &'a path::Path) -> Result<Self, Self::Error> {
        Ok(VendoredPath::new(<&camino::Utf8Path>::try_from(value)?))
    }
}

impl TryFrom<path::PathBuf> for VendoredPathBuf {
    type Error = camino::FromPathBufError;

    fn try_from(value: path::PathBuf) -> Result<Self, Self::Error> {
        Ok(VendoredPathBuf(camino::Utf8PathBuf::try_from(value)?))
    }
}

impl std::fmt::Display for VendoredPath {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "vendored://{}", &self.0)
    }
}

impl std::fmt::Display for VendoredPathBuf {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self.as_path(), f)
    }
}
