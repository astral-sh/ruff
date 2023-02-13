use std::borrow::Cow;

pub trait ToAsciiLowercaseCow {
    /// Returns the same value as String::to_lowercase. The only difference
    /// is that this functions returns ```Cow``` and does not allocate
    /// if the string is already in lowercase.
    fn to_ascii_lowercase_cow(&self) -> Cow<str>;
}

impl ToAsciiLowercaseCow for str {
    fn to_ascii_lowercase_cow(&self) -> Cow<str> {
        debug_assert!(self.is_ascii());

        let bytes = self.as_bytes();

        for idx in 0..bytes.len() {
            let chr = bytes[idx];
            if chr != chr.to_ascii_lowercase() {
                let mut s = bytes.to_vec();
                for b in &mut s[idx..] {
                    b.make_ascii_lowercase();
                }
                return Cow::Owned(unsafe { String::from_utf8_unchecked(s) });
            }
        }

        Cow::Borrowed(self)
    }
}

impl ToAsciiLowercaseCow for String {
    #[inline(always)]
    fn to_ascii_lowercase_cow(&self) -> Cow<str> {
        self.as_str().to_ascii_lowercase_cow()
    }
}

/// This signal is used to tell to the next character what it should do
#[derive(Eq, PartialEq)]
pub enum CharSignal {
    /// There hasn't been any signal
    None,
    /// The function decided to keep the previous character
    Keep,
    /// The function has decided to print the character. Saves the character that was
    /// already written
    AlreadyPrinted(char),
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub enum Quote {
    Double,
    Single,
}

impl Quote {
    pub fn as_char(&self) -> char {
        match self {
            Quote::Double => '"',
            Quote::Single => '\'',
        }
    }

    pub fn as_string(&self) -> &str {
        match self {
            Quote::Double => "\"",
            Quote::Single => "'",
        }
    }

    /// Returns the quote, prepended with a backslash (escaped)
    pub fn as_escaped(&self) -> &str {
        match self {
            Quote::Double => "\\\"",
            Quote::Single => "\\'",
        }
    }

    pub fn as_bytes(&self) -> u8 {
        self.as_char() as u8
    }

    /// Given the current quote, it returns the other one
    pub fn other(&self) -> Self {
        match self {
            Quote::Double => Quote::Single,
            Quote::Single => Quote::Double,
        }
    }
}

/// This function is responsible of:
///
/// - reducing the number of escapes
/// - normalising the new lines
///
/// # Escaping
///
/// The way it works is the following: we split the content by analyzing all the
/// characters that could keep the escape.
///
/// Each time we retrieve one of this character, we push inside a new string all the content
/// found **before** the current character.
///
/// After that the function checks if the current character should be also be printed or not.
/// These characters (like quotes) can have an escape that might be removed. If that happens,
/// we use [CharSignal] to tell to the next iteration what it should do with that character.
///
/// For example, let's take this example:
/// ```js
/// ("hello! \'")
/// ```
///
/// Here, we want to remove the backslash (\) from the content. So when we encounter `\`,
/// the algorithm checks if after `\` there's a `'`, and if so, then we push inside the final string
/// only `'` and we ignore the backlash. Then we signal the next iteration with [CharSignal::AlreadyPrinted],
/// so when we process the next `'`, we decide to ignore it and reset the signal.
///
/// Another example is the following:
///
/// ```js
/// (" \\' ")
/// ```
///
/// Here, we need to keep all the backslash. We check the first one and we look ahead. We find another
/// `\`, so we keep it the first and we signal the next iteration with [CharSignal::Keep].
/// Then the next iteration comes along. We have the second `\`, we look ahead we find a `'`. Although,
/// as opposed to the previous example, we have a signal that says that we should keep the current
/// character. Then we do so. The third iteration comes along and we find `'`. We still have the
/// [CharSignal::Keep]. We do so and then we set the signal to [CharSignal::None]
///
/// # Newlines
///
/// By default the formatter uses `\n` as a newline. The function replaces
/// `\r\n` with `\n`,
pub fn normalize_string(raw_content: &str, preferred_quote: Quote) -> Cow<str> {
    let alternate_quote = preferred_quote.other();

    // A string should be manipulated only if its raw content contains backslash or quotes
    if !raw_content.contains(['\\', preferred_quote.as_char(), alternate_quote.as_char()]) {
        return Cow::Borrowed(raw_content);
    }

    let mut reduced_string = String::new();
    let mut signal = CharSignal::None;

    let mut chars = raw_content.char_indices().peekable();

    while let Some((_, current_char)) = chars.next() {
        let next_character = chars.peek();

        if let CharSignal::AlreadyPrinted(char) = signal {
            if char == current_char {
                continue;
            }
        }

        match current_char {
            '\\' => {
                let bytes = raw_content.as_bytes();

                if let Some((next_index, next_character)) = next_character {
                    // If we encounter an alternate quote that is escaped, we have to
                    // remove the escape from it.
                    // This is done because of how the enclosed strings can change.
                    // Check `computed_preferred_quote` for more details.
                    if *next_character as u8 == alternate_quote.as_bytes()
                        // This check is a safety net for cases where the backslash is at the end
                        // of the raw content:
                        // ("\\")
                        // The second backslash is at the end.
                        && *next_index < bytes.len()
                    {
                        match signal {
                            CharSignal::Keep => {
                                reduced_string.push(current_char);
                            }
                            _ => {
                                reduced_string.push(alternate_quote.as_char());
                                signal = CharSignal::AlreadyPrinted(alternate_quote.as_char());
                            }
                        }
                    } else if signal == CharSignal::Keep {
                        reduced_string.push(current_char);
                        signal = CharSignal::None;
                    }
                    // The next character is another backslash, or
                    // a character that should be kept in the next iteration
                    else if "^\n\r\"'01234567\\bfnrtuvx\u{2028}\u{2029}".contains(*next_character)
                    {
                        signal = CharSignal::Keep;
                        // fallback, keep the backslash
                        reduced_string.push(current_char);
                    } else {
                        // these, usually characters that can have their
                        // escape removed: "\a" => "a"
                        // So we ignore the current slash and we continue
                        // to the next iteration
                        continue;
                    }
                } else {
                    // fallback, keep the backslash
                    reduced_string.push(current_char);
                }
            }
            '\n' | '\t' => {
                if let CharSignal::AlreadyPrinted(the_char) = signal {
                    if matches!(the_char, '\n' | '\t') {
                        signal = CharSignal::None
                    }
                } else {
                    reduced_string.push(current_char);
                }
            }
            // If the current character is \r and the
            // next is \n, skip over the entire sequence
            '\r' if next_character.map_or(false, |(_, c)| *c == '\n') => {
                reduced_string.push('\n');
                signal = CharSignal::AlreadyPrinted('\n');
            }
            _ => {
                // If we encounter a preferred quote and it's not escaped, we have to replace it with
                // an escaped version.
                // This is done because of how the enclosed strings can change.
                // Check `computed_preferred_quote` for more details.
                if current_char == preferred_quote.as_char() {
                    let last_char = &reduced_string.chars().last();
                    if let Some('\\') = last_char {
                        reduced_string.push(preferred_quote.as_char());
                    } else {
                        reduced_string.push_str(preferred_quote.as_escaped());
                    }
                } else if current_char == alternate_quote.as_char() {
                    match signal {
                        CharSignal::None | CharSignal::Keep => {
                            reduced_string.push(alternate_quote.as_char());
                        }
                        CharSignal::AlreadyPrinted(_) => (),
                    }
                } else {
                    reduced_string.push(current_char);
                }
                signal = CharSignal::None;
            }
        }
    }

    // Don't allocate a new string of this is empty
    if reduced_string.is_empty() {
        Cow::Borrowed(raw_content)
    } else {
        // don't allocate a new string if the new string is still equals to the input string
        if reduced_string == raw_content {
            Cow::Borrowed(raw_content)
        } else {
            Cow::Owned(reduced_string)
        }
    }
}
