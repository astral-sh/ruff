use std::path;

#[derive(Debug, Eq, PartialEq, Clone, Hash, Default)]
pub struct VendoredPathBuf(String);

impl VendoredPathBuf {
    /// Construct a new, normalized `VendoredPathBuf`.
    ///
    /// ## Panics:
    /// If a path with a prefix component is passed.
    pub fn new(path: &camino::Utf8Path) -> Self {
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
                unsupported => {
                    panic!("Unsupported component in a vendored path: {unsupported}")
                }
            }
        }
        Self(normalized_parts.join("/"))
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

impl<'a> From<&'a camino::Utf8Path> for VendoredPathBuf {
    fn from(value: &'a camino::Utf8Path) -> Self {
        VendoredPathBuf::new(value)
    }
}

impl<'a> From<&'a str> for VendoredPathBuf {
    fn from(value: &'a str) -> Self {
        VendoredPathBuf::new(<&camino::Utf8Path>::from(value))
    }
}

impl<'a> TryFrom<&'a path::Path> for VendoredPathBuf {
    type Error = camino::FromPathError;

    fn try_from(value: &'a path::Path) -> Result<Self, Self::Error> {
        Ok(VendoredPathBuf::new(<&camino::Utf8Path>::try_from(value)?))
    }
}
