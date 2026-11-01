//! Capture information for literal Python regular expressions.
//!
//! This parser follows the grouping, alternation, repetition, and lexical rules in
//! [CPython's `re._parser`](https://github.com/python/cpython/blob/v3.12.12/Lib/re/_parser.py).
//! It records capture participation directly, without compiling
//! or executing the expression. Each atom owns a contiguous range of capture indices;
//! alternatives, optional repetitions, and negative assertions make those captures optional.
//!
//! This is not a complete regex validator. Backreferences, conditionals, and late global
//! inline flags use the ordinary typeshed signatures. Character-range validity, Unicode
//! character-name lookup, and lookbehind width are not checked. Returning capture metadata
//! therefore does not imply that the pattern can be compiled by Python.

use std::ops::Range;

use bitflags::bitflags;
use ruff_python_ast::{PythonVersion, name::Name};
use ruff_python_stdlib::{identifiers::is_identifier, keyword::is_keyword};
use rustc_hash::FxHashSet;

/// Captures in a regular expression, including the whole match at index zero.
#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub struct RegexGroups {
    groups: Box<[RegexGroup]>,
}

impl RegexGroups {
    /// Analyze capture structure, falling back for unsupported or excessive patterns.
    /// Bytes patterns are passed as their Latin-1 decoding, as in CPython's tokenizer.
    pub(super) fn parse(
        pattern: &str,
        flags: i64,
        is_bytes: bool,
        python_version: PythonVersion,
    ) -> Option<Self> {
        const MAX_PATTERN_LENGTH: usize = 16_384;
        if pattern.len() > MAX_PATTERN_LENGTH {
            return None;
        }
        let flags = Flags::from_bits(u16::try_from(flags).ok()?)?;
        if !flags.valid_for_pattern(is_bytes) {
            return None;
        }
        let mut parser = Parser {
            remaining: pattern,
            is_bytes,
            python_version,
            groups: vec![RegexGroup {
                name: None,
                is_required: true,
            }],
            names: FxHashSet::default(),
        };
        parser.expression(flags, 0)?;
        parser.remaining.is_empty().then(|| Self {
            groups: parser.groups.into_boxed_slice(),
        })
    }

    pub(super) fn group(&self, index: usize) -> Option<&RegexGroup> {
        self.groups.get(index)
    }

    pub(super) fn named_group(&self, name: &str) -> Option<&RegexGroup> {
        self.groups
            .iter()
            .find(|group| group.name.as_deref() == Some(name))
    }

    /// Capturing groups in numeric order, excluding the whole match.
    pub(super) fn groups(&self) -> &[RegexGroup] {
        &self.groups[1..]
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub(super) struct RegexGroup {
    pub(super) name: Option<Name>,
    /// Whether the group participates in every successful match.
    pub(super) is_required: bool,
}

bitflags! {
    /// Python's public `re.RegexFlag` values.
    #[derive(Clone, Copy)]
    struct Flags: u16 {
        const TEMPLATE = 1;
        const IGNORE_CASE = 2;
        const LOCALE = 4;
        const MULTILINE = 8;
        const DOT_ALL = 16;
        const UNICODE = 32;
        const VERBOSE = 64;
        const DEBUG = 128;
        const ASCII = 256;
    }
}

impl Flags {
    const CHARACTER_MODE: Self = Self::ASCII.union(Self::LOCALE).union(Self::UNICODE);

    fn from_char(c: char) -> Option<Self> {
        Some(match c {
            'a' => Self::ASCII,
            'i' => Self::IGNORE_CASE,
            'L' => Self::LOCALE,
            'm' => Self::MULTILINE,
            's' => Self::DOT_ALL,
            'u' => Self::UNICODE,
            'x' => Self::VERBOSE,
            _ => return None,
        })
    }

    fn valid_for_pattern(self, is_bytes: bool) -> bool {
        if is_bytes {
            !self.contains(Self::UNICODE) && !self.contains(Self::ASCII | Self::LOCALE)
        } else {
            !self.contains(Self::LOCALE) && !self.contains(Self::ASCII | Self::UNICODE)
        }
    }
}

/// The last atom of a concatenation, to which a following quantifier applies.
struct Atom {
    captures: Range<usize>,
    kind: AtomKind,
}

#[derive(Clone, Copy)]
enum AtomKind {
    Repeatable,
    /// Anchors and boundary escapes cannot be repeated. Lookarounds can be.
    Assertion,
    Repetition,
}

enum ParsedGroup {
    Atom(Atom),
    /// Comments and global inline flags do not replace the preceding atom.
    Ignored,
}

enum BracedRepetition {
    Repeat { minimum: u32 },
    Literal,
}

enum ParsedFlags {
    Global(Flags),
    Scoped(Flags),
}

struct Parser<'a> {
    remaining: &'a str,
    is_bytes: bool,
    python_version: PythonVersion,
    groups: Vec<RegexGroup>,
    names: FxHashSet<Name>,
}

impl<'a> Parser<'a> {
    /// Parse an alternation of concatenations, stopping before a closing parenthesis.
    fn expression(&mut self, mut flags: Flags, depth: usize) -> Option<()> {
        const MAX_GROUP_DEPTH: usize = 64;
        if depth > MAX_GROUP_DEPTH {
            return None;
        }
        let capture_start = self.groups.len();
        let mut previous: Option<Atom> = None;
        let mut has_alternative = false;
        let mut has_atom = false;

        loop {
            if flags.contains(Flags::VERBOSE) {
                self.skip_verbose()?;
            }
            let Some(c) = self.peek() else { break };
            if c == ')' {
                break;
            }
            self.bump();
            match c {
                '|' => {
                    has_alternative = true;
                    previous = None;
                }
                '(' => {
                    let allow_global_flags = depth == 0 && !has_atom && !has_alternative;
                    if let ParsedGroup::Atom(atom) =
                        self.group(&mut flags, depth, allow_global_flags)?
                    {
                        previous = Some(atom);
                        has_atom = true;
                    }
                }
                '?' | '*' | '+' | '{' => {
                    let minimum = match c {
                        '?' | '*' => 0,
                        '+' => 1,
                        _ => match self.braced_repetition()? {
                            BracedRepetition::Repeat { minimum } => minimum,
                            BracedRepetition::Literal => {
                                previous = Some(self.atom(AtomKind::Repeatable));
                                has_atom = true;
                                continue;
                            }
                        },
                    };
                    let atom = previous.as_mut()?;
                    if !matches!(atom.kind, AtomKind::Repeatable) {
                        return None;
                    }
                    if minimum == 0 {
                        self.mark_optional(atom.captures.clone());
                    }
                    atom.kind = AtomKind::Repetition;
                    // Lazy and possessive suffixes must immediately follow the quantifier,
                    // even in verbose mode. Neither changes capture participation.
                    if !self.eat('?') && self.eat('+') && self.python_version < PythonVersion::PY311
                    {
                        return None;
                    }
                }
                '[' => {
                    self.character_class()?;
                    previous = Some(self.atom(AtomKind::Repeatable));
                    has_atom = true;
                }
                '\\' => {
                    let kind = self.escape(false)?;
                    previous = Some(self.atom(kind));
                    has_atom = true;
                }
                _ => {
                    let kind = if matches!(c, '^' | '$') {
                        AtomKind::Assertion
                    } else {
                        AtomKind::Repeatable
                    };
                    previous = Some(self.atom(kind));
                    has_atom = true;
                }
            }
        }
        if has_alternative {
            self.mark_optional(capture_start..self.groups.len());
        }
        Some(())
    }

    fn group(
        &mut self,
        flags: &mut Flags,
        depth: usize,
        allow_global_flags: bool,
    ) -> Option<ParsedGroup> {
        let capture_start = self.groups.len();
        let mut capture = true;
        let mut name = None;
        let mut negative = false;
        let mut inner_flags = *flags;
        if self.eat('?') {
            capture = false;
            match self.bump()? {
                'P' => {
                    // Named backreferences are deliberately unsupported.
                    if !self.eat('<') {
                        return None;
                    }
                    let group_name = self.until('>')?;
                    if !(is_identifier(group_name) || is_keyword(group_name))
                        || self.is_bytes
                            && self.python_version >= PythonVersion::PY312
                            && !group_name.is_ascii()
                    {
                        return None;
                    }
                    let group_name = Name::new(group_name);
                    if !self.names.insert(group_name.clone()) {
                        return None;
                    }
                    name = Some(group_name);
                    capture = true;
                }
                ':' | '=' => {}
                '!' => negative = true,
                '<' => match self.bump()? {
                    '=' => {}
                    '!' => negative = true,
                    _ => return None,
                },
                '>' if self.python_version >= PythonVersion::PY311 => {}
                '#' => {
                    self.skip_comment(')')?;
                    return Some(ParsedGroup::Ignored);
                }
                first @ ('a' | 'i' | 'L' | 'm' | 's' | 'u' | 'x' | '-') => {
                    match self.inline_flags(first, *flags)? {
                        ParsedFlags::Global(global) => {
                            // Before Python 3.11, late global flags can retroactively change
                            // earlier syntax. Decline those patterns instead of applying the
                            // flags only to the remainder of the expression.
                            if !allow_global_flags {
                                return None;
                            }
                            *flags = global;
                            return Some(ParsedGroup::Ignored);
                        }
                        ParsedFlags::Scoped(scoped) => inner_flags = scoped,
                    }
                }
                _ => return None,
            }
        }
        if capture {
            self.groups.push(RegexGroup {
                name,
                is_required: true,
            });
        }
        self.expression(inner_flags, depth + 1)?;
        if !self.eat(')') {
            return None;
        }
        let captures = capture_start..self.groups.len();
        if negative {
            self.mark_optional(captures.clone());
        }
        Some(ParsedGroup::Atom(Atom {
            captures,
            kind: AtomKind::Repeatable,
        }))
    }

    /// Read a count without skipping whitespace. In Python, malformed braced counts
    /// are literal text: `(a){ 0,1 }` does not make the capture optional in verbose mode.
    fn braced_repetition(&mut self) -> Option<BracedRepetition> {
        let after_open = self.remaining;
        let lower = self.digits();
        let has_comma = self.eat(',');
        let upper = if has_comma { self.digits() } else { lower };
        if !self.eat('}') || lower.is_empty() && !has_comma {
            self.remaining = after_open;
            return Some(BracedRepetition::Literal);
        }
        let minimum = if lower.is_empty() {
            0
        } else {
            lower.parse().ok()?
        };
        let maximum = if upper.is_empty() {
            u32::MAX
        } else {
            let value = upper.parse::<u32>().ok()?;
            if value == u32::MAX {
                return None;
            }
            value
        };
        if minimum == u32::MAX || minimum > maximum {
            return None;
        }
        Some(BracedRepetition::Repeat { minimum })
    }

    fn inline_flags(&mut self, first: char, outer: Flags) -> Option<ParsedFlags> {
        let mut added = Flags::empty();
        let mut removed = Flags::empty();
        let mut current = first;
        while let Some(flag) = Flags::from_char(current) {
            added.insert(flag);
            current = self.bump()?;
        }
        if (added & Flags::CHARACTER_MODE).bits().count_ones() > 1
            || !added.valid_for_pattern(self.is_bytes)
        {
            return None;
        }
        if current == ')' && !added.is_empty() {
            let combined = outer | added;
            return combined
                .valid_for_pattern(self.is_bytes)
                .then_some(ParsedFlags::Global(combined));
        }
        if current == '-' {
            current = self.bump()?;
            while let Some(flag) = Flags::from_char(current) {
                removed.insert(flag);
                current = self.bump()?;
            }
            if removed.is_empty() || removed.intersects(Flags::CHARACTER_MODE) {
                return None;
            }
        }
        if current != ':' || added.intersects(removed) {
            return None;
        }
        let mut combined = outer;
        if added.intersects(Flags::CHARACTER_MODE) {
            combined.remove(Flags::CHARACTER_MODE);
        }
        combined.insert(added);
        combined.remove(removed);
        Some(ParsedFlags::Scoped(combined))
    }

    fn character_class(&mut self) -> Option<()> {
        self.eat('^');
        let mut has_item = false;
        loop {
            match self.bump()? {
                ']' if has_item => return Some(()),
                '\\' => {
                    self.escape(true)?;
                }
                _ => {}
            }
            has_item = true;
        }
    }

    fn escape(&mut self, in_class: bool) -> Option<AtomKind> {
        let c = self.bump()?;
        match c {
            '0'..='7'
                if in_class
                    || c == '0'
                    || self
                        .remaining
                        .chars()
                        .take(2)
                        .filter(|c| matches!(c, '0'..='7'))
                        .count()
                        == 2 =>
            {
                let mut value = u32::from(c) - u32::from('0');
                for _ in 0..2 {
                    if let Some(digit @ '0'..='7') = self.peek() {
                        self.bump();
                        value = value * 8 + u32::from(digit) - u32::from('0');
                    } else {
                        break;
                    }
                }
                if value > 0xff {
                    return None;
                }
            }
            // A one- or two-digit nonzero escape is a group reference. It can impose
            // additional participation constraints, which are not analyzed here.
            '0'..='9' => return None,
            'x' => {
                self.hex_escape(2)?;
            }
            'u' | 'U' if !self.is_bytes => {
                let value = self.hex_escape(if c == 'u' { 4 } else { 8 })?;
                if value > 0x0010_ffff {
                    return None;
                }
            }
            'N' if !self.is_bytes && self.python_version >= PythonVersion::PY38 => {
                if !self.eat('{') || self.until('}')?.is_empty() {
                    return None;
                }
            }
            'A' | 'B' | 'Z' if !in_class => return Some(AtomKind::Assertion),
            'z' if !in_class && self.python_version >= PythonVersion::PY314 => {
                return Some(AtomKind::Assertion);
            }
            'b' if !in_class => return Some(AtomKind::Assertion),
            'a' | 'b' | 'f' | 'n' | 'r' | 't' | 'v' | 'd' | 'D' | 's' | 'S' | 'w' | 'W' => {}
            _ if c.is_ascii_alphabetic() => return None,
            _ => {}
        }
        Some(AtomKind::Repeatable)
    }

    fn hex_escape(&mut self, length: usize) -> Option<u32> {
        let mut value = 0;
        for _ in 0..length {
            value = value * 16 + self.bump()?.to_digit(16)?;
        }
        Some(value)
    }

    fn skip_verbose(&mut self) -> Option<()> {
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c') => {
                    self.bump();
                }
                Some('#') => {
                    self.skip_comment('\n')?;
                }
                _ => return Some(()),
            }
        }
    }

    /// Python's tokenizer treats an escape as one token even inside comments.
    /// An escaped newline therefore continues a verbose comment onto the next line.
    fn skip_comment(&mut self, terminator: char) -> Option<()> {
        while let Some(c) = self.bump() {
            if c == terminator {
                return Some(());
            }
            if c == '\\' {
                self.bump()?;
            }
        }
        (terminator == '\n').then_some(())
    }

    fn mark_optional(&mut self, captures: Range<usize>) {
        for group in &mut self.groups[captures] {
            group.is_required = false;
        }
    }

    fn atom(&self, kind: AtomKind) -> Atom {
        Atom {
            captures: self.groups.len()..self.groups.len(),
            kind,
        }
    }

    fn digits(&mut self) -> &'a str {
        let start = self.remaining;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
        &start[..start.len() - self.remaining.len()]
    }

    fn until(&mut self, terminator: char) -> Option<&'a str> {
        let start = self.remaining;
        let length = start.find(terminator)?;
        self.remaining = &start[length + terminator.len_utf8()..];
        Some(&start[..length])
    }

    fn peek(&self) -> Option<char> {
        self.remaining.chars().next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.remaining = &self.remaining[c.len_utf8()..];
        Some(c)
    }

    fn eat(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.bump();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::PythonVersion;

    use super::RegexGroups;

    fn parse(pattern: &str) -> Option<RegexGroups> {
        RegexGroups::parse(pattern, 0, false, PythonVersion::PY312)
    }

    fn assert_captures(pattern: &str, flags: i64, required: &[bool]) {
        let groups = RegexGroups::parse(pattern, flags, false, PythonVersion::PY312)
            .unwrap_or_else(|| panic!("could not parse {pattern:?}"));
        assert!(groups.group(0).is_some_and(|group| group.is_required));
        assert_eq!(
            groups
                .groups()
                .iter()
                .map(|group| group.is_required)
                .collect::<Vec<_>>(),
            required,
            "{pattern:?}",
        );
    }

    #[test]
    fn capture_participation() {
        for (pattern, required) in [
            ("abc", vec![]),
            ("(a)(?:b)(?P<name>c)", vec![true, true]),
            ("((a)|(b))", vec![true, false, false]),
            ("(a)|(b)", vec![false, false]),
            (
                "(a)?(b)*(c)+(d){0,2}(e){1,2}",
                vec![false, false, true, false, true],
            ),
            ("(a){,2}(b){,}(c)++(d)?+", vec![false, false, true, false]),
            ("(a)+?(b)*?(c)??", vec![true, false, false]),
            ("((a)?)+", vec![true, false]),
            (
                "(?=(a))(?<=(b))(?!c(d))(?<!e(f))(g)",
                vec![true, true, false, false, true],
            ),
            ("(?!(?=(a)))(b)", vec![false, true]),
            ("(?=(a))?", vec![false]),
            ("(?>a(b))", vec![true]),
        ] {
            assert_captures(pattern, 0, &required);
        }
    }

    #[test]
    fn literal_braces_and_counts() {
        for pattern in [
            "{(a)}",
            "(a){word}",
            "(a){}",
            "(a){",
            "(a){999999999999999999999999999999x}",
            "(a){1}",
            "(a){1,}",
        ] {
            assert_captures(pattern, 0, &[true]);
        }
        for pattern in ["(a){0}", "(a){0,1}", "(a){,1}", "(a){,}"] {
            assert_captures(pattern, 0, &[false]);
        }
        for pattern in ["(a){ 0,1 }", "(a){0, 1}", "(a){, 1}", "(a){ ,1}"] {
            assert_captures(pattern, 0x40, &[true]);
        }
    }

    #[test]
    fn escaped_atoms_and_character_classes() {
        for pattern in [
            r"\((a)\)",
            r"\N{LEFT PARENTHESIS}(a)",
            r"\x28\u0028\U00000028(a)",
            r"(a)\0\077\141\377",
            r"[\1\77\141](a)",
            r"[^]()]([]()])",
            r"[[]([()])",
            r"[\N{RIGHT SQUARE BRACKET}](a)",
            r"[\x5d\u005d\U0000005d](a)",
        ] {
            assert_captures(pattern, 0, &[true]);
        }
    }

    #[test]
    fn named_groups() {
        let groups = parse("(?P<℘>a)(?P<a\u{301}>b)?(?P<class>c)").unwrap();
        assert!(groups.named_group("℘").unwrap().is_required);
        assert!(!groups.named_group("a\u{301}").unwrap().is_required);
        assert!(groups.named_group("class").unwrap().is_required);
        assert!(groups.named_group("missing").is_none());
        assert!(groups.group(4).is_none());
    }

    #[test]
    fn comments_and_verbose_mode() {
        for pattern in [
            "(a) # (ignored)\n(b)",
            "(?x)(a) # (ignored)\n(b)",
            "(?x)(a) # (ignored)\\\n(also ignored)\n(b)",
            "(?x)(a) # (ignored)\\\\\n(b)",
            "(?x:(a) # (ignored)\n)(b)",
            "(?x:(a))(?-x:#(b))",
            "(?x)[#](a)\\#(b)",
        ] {
            assert_captures(pattern, 0x40, &[true, true]);
        }
        assert_captures("(?x:(a))#(b)", 0, &[true, true]);
        assert_captures("(?x)(?-x:#(a)) # (ignored)", 0, &[true]);
        assert_captures("(?x)(a)\u{85}?", 0, &[true]);
        assert_captures(r"(?#ignored ( and \))(a)", 0, &[true]);
        assert_captures("(a)(?#comment)?", 0, &[false]);
        assert_captures("(a)(?#comment)*", 0, &[false]);
        assert_captures("(?x)(a) #comment\n ?", 0, &[false]);
        assert_captures("(?#comment)(?x)(a)", 0, &[true]);
        assert_captures("(?i)(?x)(a)", 0, &[true]);
    }

    #[test]
    fn flags_and_python_versions() {
        assert_captures("(?a:(?u:(a)))", 0, &[true]);
        assert!(RegexGroups::parse("(?a:(?L:(a)))", 0, true, PythonVersion::PY312).is_some());
        for pattern in ["(a)++", "(a)?+", "(a){1,2}+", "(?>a(b))"] {
            assert!(RegexGroups::parse(pattern, 0, false, PythonVersion::PY310).is_none());
            assert!(RegexGroups::parse(pattern, 0, false, PythonVersion::PY311).is_some());
        }
        assert!(RegexGroups::parse(r"\N{SPACE}(a)", 0, false, PythonVersion::PY37).is_none());
        assert!(RegexGroups::parse(r"\N{SPACE}(a)", 0, false, PythonVersion::PY38).is_some());
        assert!(RegexGroups::parse(r"(a)\z", 0, false, PythonVersion::PY313).is_none());
        assert!(RegexGroups::parse(r"(a)\z", 0, false, PythonVersion::PY314).is_some());
        assert!(RegexGroups::parse("(?P<é>a)", 0, true, PythonVersion::PY311).is_some());
        assert!(RegexGroups::parse("(?P<é>a)", 0, true, PythonVersion::PY312).is_none());
        for pattern in [r"\N{SPACE}(a)", r"\u0061(a)", r"\U00000061(a)"] {
            assert!(RegexGroups::parse(pattern, 0, true, PythonVersion::PY312).is_none());
        }
        for (flags, is_bytes) in [(4, false), (32, true), (256 | 4, true), (256 | 32, false)] {
            assert!(RegexGroups::parse("(a)", flags, is_bytes, PythonVersion::PY312).is_none());
        }
        // Late global flags are rejected in every target version. Older Python versions
        // apply them retroactively, which can change the number of earlier captures.
        for version in [PythonVersion::PY310, PythonVersion::PY312] {
            for pattern in [
                "(a) # (fake)\n(b)?(?x)",
                "a|(?x)(b)",
                "|(?x)(a)",
                "(a)(?i)",
                "(?:(?x)a)",
            ] {
                assert!(RegexGroups::parse(pattern, 0, false, version).is_none());
            }
        }
    }

    #[test]
    fn unsupported_and_invalid_patterns() {
        for pattern in [
            "(",
            ")",
            "[",
            "[]",
            r"\",
            r"(a)\1",
            r"(a)\12",
            r"(a)\777",
            r"(a)\U00110000",
            "(?P<a>x)(?P=a)",
            "(a)(?(1)b|c)",
            "(?P<a>x)(?P<a>y)",
            "(?P<1>a)",
            "(a){2,1}",
            "(a){4294967295}",
            "*",
            "^+",
            "(a)**",
            "(?x)(a)* ?",
            "(?a-u:(a))",
            "(?au:(a))",
            "(?i-i:(a))",
            "(?-:(a))",
        ] {
            assert!(parse(pattern).is_none(), "{pattern:?}");
        }
        assert!(RegexGroups::parse("(a)", 0x200, false, PythonVersion::PY312).is_none());
        assert!(RegexGroups::parse("(a)", -1, false, PythonVersion::PY312).is_none());
    }

    #[test]
    fn resource_limits() {
        assert!(parse(&format!("{}a{}", "(".repeat(64), ")".repeat(64))).is_some());
        assert!(parse(&format!("{}a{}", "(".repeat(65), ")".repeat(65))).is_none());
        assert!(parse(&"a".repeat(16_384)).is_some());
        assert!(parse(&"a".repeat(16_385)).is_none());
        assert_eq!(parse(&"()".repeat(8_192)).unwrap().groups().len(), 8_192);
    }
}
