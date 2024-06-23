use std::fmt;
use std::path;

#[derive(Debug, Eq, PartialEq, Clone, Hash, Default)]
pub struct VendoredPathBuf(String);

impl VendoredPathBuf {
    pub fn new(path: &camino::Utf8Path) -> Result<Self, VendoredPathConstructionError> {
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
                    if normalized_parts.pop().is_none() {
                        return Err(VendoredPathConstructionError::EscapeFromZipArchive);
                    }
                }
                unsupported => {
                    return Err(VendoredPathConstructionError::UnsupportedComponent(
                        unsupported,
                    ))
                }
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
    type Error = VendoredPathConstructionError<'a>;

    fn try_from(value: &'a camino::Utf8Path) -> Result<Self, Self::Error> {
        VendoredPathBuf::new(value)
    }
}

impl<'a> TryFrom<&'a str> for VendoredPathBuf {
    type Error = VendoredPathConstructionError<'a>;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        VendoredPathBuf::new(camino::Utf8Path::new(value))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum VendoredPathConstructionError<'a> {
    InvalidUTF8(&'a path::Path),
    UnsupportedComponent(camino::Utf8Component<'a>),
    EscapeFromZipArchive,
}

impl<'a> fmt::Display for VendoredPathConstructionError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUTF8(path) => write!(f, "Could not convert {path:?} to a UTF-8 string"),
            Self::UnsupportedComponent(component) => {
                write!(f, "Unsupported path component {component}")
            }
            Self::EscapeFromZipArchive => {
                f.write_str("Path attempts to escape out of the zip archive using `..` parts")
            }
        }
    }
}

impl<'a> std::error::Error for VendoredPathConstructionError<'a> {}

impl<'a> TryFrom<&'a path::Path> for VendoredPathBuf {
    type Error = VendoredPathConstructionError<'a>;

    fn try_from(value: &'a path::Path) -> Result<Self, Self::Error> {
        let Some(path_str) = value.to_str() else {
            return Err(VendoredPathConstructionError::InvalidUTF8(value));
        };
        VendoredPathBuf::new(camino::Utf8Path::new(path_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_path_escaping_from_zip_archive() {
        assert_eq!(
            VendoredPathBuf::try_from(".."),
            Err(VendoredPathConstructionError::EscapeFromZipArchive)
        );
    }

    #[test]
    fn fancy_path_escaping_from_zip_archive() {
        assert_eq!(
            VendoredPathBuf::try_from("./foo/../../../foo"),
            Err(VendoredPathConstructionError::EscapeFromZipArchive)
        );
    }
}
