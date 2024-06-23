use std::fmt;
use std::path;

#[derive(Debug)]
pub struct UnsupportedComponentError(String);

impl fmt::Display for UnsupportedComponentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Unsupported component in a vendored path: {:?}", self.0)
    }
}

impl std::error::Error for UnsupportedComponentError {}

#[derive(Debug, Eq, PartialEq, Clone, Hash, Default)]
pub struct VendoredPathBuf(String);

impl VendoredPathBuf {
    pub fn new(path: &camino::Utf8Path) -> Result<Self, UnsupportedComponentError> {
        // Allow the `RootDir` component, but only if it is at the very start of the string.
        let mut components = path.components().peekable();
        if let Some(camino::Utf8Component::RootDir) = components.peek() {
            components.next();
        }

        let mut normalized_parts = Vec::new();

        for component in components {
            match component {
                camino::Utf8Component::Normal(part) => normalized_parts.push(part),
                camino::Utf8Component::CurDir => continue,
                camino::Utf8Component::ParentDir => {
                    normalized_parts.pop();
                }
                unsupported => return Err(UnsupportedComponentError(unsupported.to_string())),
            }
        }
        Ok(Self(normalized_parts.join("/")))
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn as_std_path(&self) -> &path::Path {
        path::Path::new(&self.0)
    }

    pub fn components(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }
}

impl<'a> TryFrom<&'a camino::Utf8Path> for VendoredPathBuf {
    type Error = UnsupportedComponentError;

    fn try_from(value: &'a camino::Utf8Path) -> Result<Self, Self::Error> {
        VendoredPathBuf::new(value)
    }
}

impl<'a> TryFrom<&'a str> for VendoredPathBuf {
    type Error = UnsupportedComponentError;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        VendoredPathBuf::new(camino::Utf8Path::new(value))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VendoredPathConstructionError<'a> {
    #[error("Could not convert {0} to a UTF-8 string")]
    InvalidUTF8(&'a path::Path),
    #[error("{0}")]
    UnsupporteComponent(#[from] UnsupportedComponentError),
}

impl<'a> TryFrom<&'a path::Path> for VendoredPathBuf {
    type Error = VendoredPathConstructionError<'a>;

    fn try_from(value: &'a path::Path) -> Result<Self, Self::Error> {
        let Some(path_str) = value.to_str() else {
            return Err(VendoredPathConstructionError::InvalidUTF8(value));
        };
        Ok(VendoredPathBuf::new(camino::Utf8Path::new(path_str))?)
    }
}
