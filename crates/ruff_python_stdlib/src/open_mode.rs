bitflags::bitflags! {
    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    pub struct OpenMode: u8 {
        /// `r`
        const READ = 1 << 0;
        /// `w`
        const WRITE = 1 << 1;
        /// `a`
        const APPEND = 1 << 2;
        /// `x`
        const CREATE = 1 << 3;
        /// `b`
        const BINARY = 1 << 4;
        /// `t`
        const TEXT = 1 << 5;
        /// `+`
        const PLUS = 1 << 6;
        /// `U`
        const UNIVERSAL_NEWLINES = 1 << 7;
    }
}

impl OpenMode {
    /// Parse an [`OpenMode`] from a sequence of characters.
    pub fn from_chars(chars: impl Iterator<Item = char>) -> Result<Self, String> {
        let mut open_mode = OpenMode::empty();
        for c in chars {
            let flag = OpenMode::try_from(c)?;
            if flag.intersects(open_mode) {
                return Err(format!("Open mode contains duplicate flag: `{c}`"));
            }
            open_mode.insert(flag);
        }

        // Both text and binary mode cannot be set at the same time.
        if open_mode.contains(OpenMode::TEXT | OpenMode::BINARY) {
            return Err(
                "Open mode cannot contain both text (`t`) and binary (`b`) flags".to_string(),
            );
        }

        // The `U` mode is only valid with `r`.
        if open_mode.contains(OpenMode::UNIVERSAL_NEWLINES)
            && open_mode.intersects(OpenMode::WRITE | OpenMode::APPEND | OpenMode::CREATE)
        {
            return Err("Open mode cannot contain the universal newlines (`U`) flag with write (`w`), append (`a`), or create (`x`) flags".to_string());
        }

        // Otherwise, reading, writing, creating, and appending are mutually exclusive.
        if [
            OpenMode::READ | OpenMode::UNIVERSAL_NEWLINES,
            OpenMode::WRITE,
            OpenMode::CREATE,
            OpenMode::APPEND,
        ]
        .into_iter()
        .filter(|flag| open_mode.intersects(*flag))
        .count()
            != 1
        {
            return Err("Open mode must contain exactly one of the following flags: read (`r`), write (`w`), create (`x`), or append (`a`)".to_string());
        }

        Ok(open_mode)
    }

    /// Remove any redundant flags from the open mode.
    #[must_use]
    pub fn reduce(self) -> Self {
        let mut open_mode = self;

        // `t` is always redundant.
        open_mode.remove(Self::TEXT);

        // `U` is always redundant.
        open_mode.remove(Self::UNIVERSAL_NEWLINES);

        // `r` is redundant, unless `b` or `+` is also set, in which case, we need one of `w`, `a`, `r`, or `x`.
        if open_mode.intersects(Self::BINARY | Self::PLUS) {
            if !open_mode.intersects(Self::WRITE | Self::CREATE | Self::APPEND) {
                open_mode.insert(Self::READ);
            }
        } else {
            open_mode.remove(Self::READ);
        }

        open_mode
    }
}

/// Write the [`OpenMode`] as a string.
impl std::fmt::Display for OpenMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.contains(OpenMode::READ) {
            write!(f, "r")?;
        }
        if self.contains(OpenMode::WRITE) {
            write!(f, "w")?;
        }
        if self.contains(OpenMode::APPEND) {
            write!(f, "a")?;
        }
        if self.contains(OpenMode::CREATE) {
            write!(f, "x")?;
        }
        if self.contains(OpenMode::UNIVERSAL_NEWLINES) {
            write!(f, "U")?;
        }
        if self.contains(OpenMode::BINARY) {
            write!(f, "b")?;
        }
        if self.contains(OpenMode::TEXT) {
            write!(f, "t")?;
        }
        if self.contains(OpenMode::PLUS) {
            write!(f, "+")?;
        }
        Ok(())
    }
}

impl TryFrom<char> for OpenMode {
    type Error = String;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        match value {
            'r' => Ok(Self::READ),
            'w' => Ok(Self::WRITE),
            'a' => Ok(Self::APPEND),
            'x' => Ok(Self::CREATE),
            'b' => Ok(Self::BINARY),
            't' => Ok(Self::TEXT),
            '+' => Ok(Self::PLUS),
            'U' => Ok(Self::UNIVERSAL_NEWLINES),
            _ => Err(format!("Invalid open mode flag: `{value}`")),
        }
    }
}

impl TryFrom<&str> for OpenMode {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        OpenMode::from_chars(value.chars())
    }
}
