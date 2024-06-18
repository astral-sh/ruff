use std::ops::Deref;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Name(smol_str::SmolStr);

impl Name {
    #[inline]
    pub fn new(name: &str) -> Self {
        Self(smol_str::SmolStr::new(name))
    }

    #[inline]
    pub fn new_static(name: &'static str) -> Self {
        Self(smol_str::SmolStr::new_static(name))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Deref for Name {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl<T> From<T> for Name
where
    T: Into<smol_str::SmolStr>,
{
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl PartialEq<str> for Name {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<Name> for str {
    fn eq(&self, other: &Name) -> bool {
        other == self
    }
}
