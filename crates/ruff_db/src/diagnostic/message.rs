use std::cell::RefCell;
use std::fmt::{self, Write};

// `fmt::Formatter` does not expose its underlying writer, so semantic `Display` implementations
// need a scoped transport to associate a name with the diagnostic message currently being written.
// This is not persistent Salsa query state: each synchronous `from_display` call owns one stack
// frame, nested calls receive independent frames, and `Drop` removes a frame even during unwinding.
// Captured names contain only owned data and lifetime-free Salsa identities.
thread_local! {
    static MESSAGE_CAPTURES: RefCell<Vec<MessageCapture>> = const { RefCell::new(Vec::new()) };
}

/// The semantic category of a named item embedded in a diagnostic message.
///
/// Classes and aliases can have the same spelling while denoting different items. Their category
/// determines which semantic value is reconstructed from its Salsa identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub enum DiagnosticNameKind {
    Class,
    TypeAlias,
}

/// The spelling and semantic identity of a name displayed in a diagnostic.
///
/// The identity carries no database lifetime, allowing inference queries to retain unresolved
/// diagnostics without reading another file's syntax tree. It must be resolved against its
/// originating database. Qualified names and source locations are requested only when the
/// completed diagnostic actually needs them.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub struct DiagnosticName {
    spelling: Box<str>,
    #[get_size(ignore)]
    identity: salsa::Id,
    kind: DiagnosticNameKind,
}

impl DiagnosticName {
    pub fn new(
        spelling: impl Into<Box<str>>,
        identity: salsa::Id,
        kind: DiagnosticNameKind,
    ) -> Self {
        Self {
            spelling: spelling.into(),
            identity,
            kind,
        }
    }

    pub(super) fn spelling(&self) -> &str {
        &self.spelling
    }

    /// Returns the underlying Salsa identity for this named item.
    pub fn identity(&self) -> salsa::Id {
        self.identity
    }

    /// Returns the semantic category needed to reconstruct this named item.
    pub fn kind(&self) -> DiagnosticNameKind {
        self.kind
    }

    pub(super) fn has_same_identity(&self, other: &Self) -> bool {
        self.identity == other.identity && self.kind == other.kind
    }
}

/// Resolves presentation details only for genuinely ambiguous diagnostic names.
pub trait DiagnosticNameResolver {
    /// Returns the fully qualified spelling of this named item.
    fn qualified_name(&self, name: &DiagnosticName) -> String;

    /// Returns a source-location suffix when qualified spellings are also identical.
    fn location(&self, name: &DiagnosticName) -> String;
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub(super) struct DiagnosticNameOccurrence {
    start: usize,
    end: usize,
    name: DiagnosticName,
    replace_when_ambiguous: bool,
}

impl DiagnosticNameOccurrence {
    pub(super) fn name(&self) -> &DiagnosticName {
        &self.name
    }
}

#[derive(Debug, Default)]
struct MessageCapture {
    offset: usize,
    names: Vec<DiagnosticNameOccurrence>,
}

/// A semantic name and the formatting operation that displays it.
///
/// The named fields distinguish the formatting operation from its lazily constructed metadata.
#[derive(Debug)]
pub struct DiagnosticNameRecord<Render, Name> {
    /// Writes the name into the current formatter.
    pub render: Render,
    /// Constructs identifying metadata only when a diagnostic message records this output.
    pub name: Name,
}

impl<Render, Name> DiagnosticNameRecord<Render, Name>
where
    Render: FnOnce() -> fmt::Result,
    Name: FnOnce() -> DiagnosticName,
{
    /// Records this name when its formatting operation writes into a diagnostic message.
    ///
    /// Ordinary displays, including IDE displays and tests, do not construct semantic metadata.
    pub fn render(self) -> fmt::Result {
        self.render_impl(true)
    }

    /// Records a semantic name without changing its displayed spelling during disambiguation.
    ///
    /// For example, `<module 'os'>` identifies Python's module class. Its `module` spelling should
    /// disambiguate a user-defined `module` class without changing the conventional representation.
    pub fn render_fixed(self) -> fmt::Result {
        self.render_impl(false)
    }

    fn render_impl(self, replace_when_ambiguous: bool) -> fmt::Result {
        let Self { render, name } = self;
        // Either callback can evaluate Salsa queries or format another diagnostic, so no borrow
        // of the capture stack may remain active while a callback runs.
        let Some(start) = MESSAGE_CAPTURES
            .with(|captures| captures.borrow().last().map(|capture| capture.offset))
        else {
            return render();
        };

        render()?;

        let end = MESSAGE_CAPTURES
            .with(|captures| captures.borrow().last().map(|capture| capture.offset));

        // A nested `format!` writes into its own buffer, not this message's writer. Its output
        // cannot be associated with a range until the caller preserves it as a DiagnosticMessage.
        if let Some(end) = end
            && end > start
        {
            let occurrence = DiagnosticNameOccurrence {
                start,
                end,
                name: name(),
                replace_when_ambiguous,
            };
            MESSAGE_CAPTURES.with(|captures| {
                if let Some(capture) = captures.borrow_mut().last_mut() {
                    capture.names.push(occurrence);
                }
            });
        }

        Ok(())
    }
}

// This guard owns its stack frame. Cloning or copying it would let multiple guards remove the
// same frame, so it deliberately does not implement `Clone` or `Copy`.
#[derive(Debug, Eq, PartialEq)]
struct MessageCaptureScope;

impl MessageCaptureScope {
    fn new() -> Self {
        MESSAGE_CAPTURES.with(|captures| captures.borrow_mut().push(MessageCapture::default()));
        Self
    }

    fn finish(self) -> Vec<DiagnosticNameOccurrence> {
        MESSAGE_CAPTURES.with(|captures| {
            captures
                .borrow_mut()
                .last_mut()
                .map(|capture| std::mem::take(&mut capture.names))
                .unwrap_or_default()
        })
    }
}

impl Drop for MessageCaptureScope {
    fn drop(&mut self) {
        MESSAGE_CAPTURES.with(|captures| captures.borrow_mut().pop());
    }
}

#[derive(Debug, Default)]
struct DiagnosticMessageWriter {
    message: String,
}

impl Write for DiagnosticMessageWriter {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.message.push_str(text);
        MESSAGE_CAPTURES.with(|captures| {
            if let Some(capture) = captures.borrow_mut().last_mut() {
                capture.offset += text.len();
            }
        });
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
struct StructuredDiagnosticMessage {
    text: Box<str>,
    names: Box<[DiagnosticNameOccurrence]>,
}

impl StructuredDiagnosticMessage {
    fn resolve_names(&self, qualification: &dyn Fn(&DiagnosticName) -> Option<String>) -> Box<str> {
        let mut text = self.text.to_string();
        let mut occurrences: Vec<_> = self.names.iter().collect();
        occurrences.sort_unstable_by_key(|occurrence| occurrence.start);

        for occurrence in occurrences.into_iter().rev() {
            if occurrence.replace_when_ambiguous
                && let Some(replacement) = qualification(&occurrence.name)
                && occurrence.end <= text.len()
                && text.is_char_boundary(occurrence.start)
                && text.is_char_boundary(occurrence.end)
            {
                text.replace_range(occurrence.start..occurrence.end, &replacement);
            }
        }

        text.into_boxed_str()
    }
}

/// Separate message text is retained only when full and concise qualification actually differ.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
struct DiagnosticMessageVariants {
    full: Box<str>,
    concise: Box<str>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
enum DiagnosticMessageRepr {
    Plain(Box<str>),
    Structured(Box<StructuredDiagnosticMessage>),
    Variants(Box<DiagnosticMessageVariants>),
}

pub(super) type DiagnosticNameQualification<'a> = &'a dyn Fn(&DiagnosticName) -> Option<String>;

/// A diagnostic message that may temporarily retain semantic names until finalization.
///
/// Finalized messages contain only their owned text. The value intentionally does not implement
/// `Display`, allowing `IntoDiagnosticMessage` to move an existing message without copying its text
/// or losing its metadata. Other displayable values use the trait's blanket implementation.
#[derive(Clone, Debug, Eq, PartialEq, Hash, get_size2::GetSize)]
pub struct DiagnosticMessage(DiagnosticMessageRepr);

impl DiagnosticMessage {
    /// Formats a message while retaining the identities of any names emitted by its arguments.
    pub fn from_display(display: impl fmt::Display) -> Self {
        let capture = MessageCaptureScope::new();
        let mut writer = DiagnosticMessageWriter::default();
        let result = write!(&mut writer, "{display}");
        let names = capture.finish();

        if result.is_err() || names.is_empty() {
            return Self::from(writer.message);
        }

        Self(DiagnosticMessageRepr::Structured(Box::new(
            StructuredDiagnosticMessage {
                text: writer.message.into_boxed_str(),
                names: names.into_boxed_slice(),
            },
        )))
    }

    /// Returns this message as a borrowed string.
    pub fn as_str(&self) -> &str {
        match &self.0 {
            DiagnosticMessageRepr::Plain(text) => text,
            DiagnosticMessageRepr::Structured(message) => &message.text,
            DiagnosticMessageRepr::Variants(message) => &message.full,
        }
    }

    pub(super) fn as_concise_str(&self) -> &str {
        match &self.0 {
            DiagnosticMessageRepr::Variants(message) => &message.concise,
            DiagnosticMessageRepr::Plain(text) => text,
            DiagnosticMessageRepr::Structured(message) => &message.text,
        }
    }

    /// Prepends text without losing the semantic ranges already recorded in this message.
    #[must_use]
    pub fn with_prefix(self, prefix: &str) -> Self {
        if prefix.is_empty() {
            return self;
        }

        match self.0 {
            DiagnosticMessageRepr::Plain(text) => Self::from(format!("{prefix}{text}")),
            DiagnosticMessageRepr::Structured(mut message) => {
                message.text = format!("{prefix}{}", message.text).into_boxed_str();
                for occurrence in &mut message.names {
                    occurrence.start += prefix.len();
                    occurrence.end += prefix.len();
                }
                Self(DiagnosticMessageRepr::Structured(message))
            }
            DiagnosticMessageRepr::Variants(mut message) => {
                message.full = format!("{prefix}{}", message.full).into_boxed_str();
                message.concise = format!("{prefix}{}", message.concise).into_boxed_str();
                Self(DiagnosticMessageRepr::Variants(message))
            }
        }
    }

    pub(super) fn names(&self) -> &[DiagnosticNameOccurrence] {
        match &self.0 {
            DiagnosticMessageRepr::Plain(_) | DiagnosticMessageRepr::Variants(_) => &[],
            DiagnosticMessageRepr::Structured(message) => &message.names,
        }
    }

    pub(super) fn resolve_names(
        &mut self,
        full_qualification: impl Fn(&DiagnosticName) -> Option<String>,
        concise_qualification: Option<DiagnosticNameQualification<'_>>,
    ) {
        let DiagnosticMessageRepr::Structured(message) = &self.0 else {
            return;
        };

        let full = message.resolve_names(&full_qualification);
        let concise = concise_qualification
            .map(|qualification| message.resolve_names(qualification))
            .filter(|concise| concise != &full);

        self.0 = if let Some(concise) = concise {
            DiagnosticMessageRepr::Variants(Box::new(DiagnosticMessageVariants { full, concise }))
        } else {
            DiagnosticMessageRepr::Plain(full)
        };
    }
}

impl From<&str> for DiagnosticMessage {
    fn from(text: &str) -> Self {
        Self(DiagnosticMessageRepr::Plain(text.into()))
    }
}

impl From<String> for DiagnosticMessage {
    fn from(text: String) -> Self {
        Self(DiagnosticMessageRepr::Plain(text.into_boxed_str()))
    }
}

impl From<Box<str>> for DiagnosticMessage {
    fn from(text: Box<str>) -> Self {
        Self(DiagnosticMessageRepr::Plain(text))
    }
}

impl IntoDiagnosticMessage for DiagnosticMessage {
    fn into_diagnostic_message(self) -> DiagnosticMessage {
        self
    }
}

/// Converts either an existing message or any displayable value into a diagnostic message.
///
/// Existing messages can be moved without copying their text or discarding semantic metadata,
/// while the blanket implementation accepts ordinary formatting arguments such as `format_args!`.
pub trait IntoDiagnosticMessage {
    fn into_diagnostic_message(self) -> DiagnosticMessage;
}

impl<T: fmt::Display> IntoDiagnosticMessage for T {
    fn into_diagnostic_message(self) -> DiagnosticMessage {
        DiagnosticMessage::from_display(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{
        Annotation, Diagnostic, DiagnosticId, Severity, Span, SubDiagnostic, SubDiagnosticSeverity,
    };
    use crate::files::{File, system_path_to_file};
    use crate::system::DbWithWritableSystem;
    use crate::tests::TestDb;
    use salsa::plumbing::AsId;
    use std::cell::Cell;
    use std::error::Error;

    fn files() -> Result<(TestDb, File, File), Box<dyn Error>> {
        let mut db = TestDb::default();
        db.write_file("/first.py", "class Model: ...")?;
        db.write_file("/second.py", "class Model: ...")?;

        let first = system_path_to_file(&db, "/first.py")?;
        let second = system_path_to_file(&db, "/second.py")?;
        Ok((db, first, second))
    }

    fn name(file: File, spelling: &'static str) -> impl fmt::Display {
        fmt::from_fn(move |f| {
            DiagnosticNameRecord {
                render: || f.write_str(spelling),
                name: || DiagnosticName::new(spelling, file.as_id(), DiagnosticNameKind::Class),
            }
            .render()
        })
    }

    struct TestNameResolver {
        names: Vec<(File, &'static str)>,
        qualified_calls: Cell<usize>,
        location_calls: Cell<usize>,
    }

    impl TestNameResolver {
        fn new(names: &[(File, &'static str)]) -> Self {
            Self {
                names: names.to_vec(),
                qualified_calls: Cell::new(0),
                location_calls: Cell::new(0),
            }
        }
    }

    impl DiagnosticNameResolver for TestNameResolver {
        fn qualified_name(&self, name: &DiagnosticName) -> String {
            self.qualified_calls.set(self.qualified_calls.get() + 1);
            self.names
                .iter()
                .find(|(file, _)| file.as_id() == name.identity())
                .map(|(_, qualified)| (*qualified).to_owned())
                .unwrap_or_else(|| panic!("missing qualified name for {:?}", name.identity()))
        }

        fn location(&self, _name: &DiagnosticName) -> String {
            self.location_calls.set(self.location_calls.get() + 1);
            " @ definition".to_owned()
        }
    }

    #[salsa::tracked(returns(clone))]
    fn cached_nested_message(db: &dyn crate::Db, file: File) -> String {
        let _ = file.path(db);
        DiagnosticMessage::from_display(format_args!("Nested `{}`", name(file, "Model")))
            .as_str()
            .to_owned()
    }

    #[test]
    fn ordinary_formatting_does_not_construct_diagnostic_metadata() -> Result<(), Box<dyn Error>> {
        let (_db, first, _second) = files()?;
        let metadata_created = Cell::new(false);
        let display = fmt::from_fn(|f| {
            DiagnosticNameRecord {
                render: || f.write_str("Model"),
                name: || {
                    metadata_created.set(true);
                    DiagnosticName::new("Model", first.as_id(), DiagnosticNameKind::Class)
                },
            }
            .render()
        });

        assert_eq!(display.to_string(), "Model");
        assert!(!metadata_created.get());
        Ok(())
    }

    #[test]
    fn nested_message_capture_survives_cold_and_cached_salsa_queries() -> Result<(), Box<dyn Error>>
    {
        let (db, first, second) = files()?;

        for _ in 0..2 {
            let outer = fmt::from_fn(|f| {
                DiagnosticNameRecord {
                    render: || {
                        let nested = cached_nested_message(&db, first);
                        assert_eq!(nested, "Nested `Model`");
                        f.write_str("Model")
                    },
                    name: || {
                        DiagnosticName::new("Model", second.as_id(), DiagnosticNameKind::Class)
                    },
                }
                .render()
            });
            let mut diagnostic = Diagnostic::new(
                DiagnosticId::InvalidSyntax,
                Severity::Error,
                format_args!("Expected `{outer}`"),
            );
            diagnostic.info(format_args!("Found `{}`", name(first, "Model")));

            diagnostic.disambiguate_names(&TestNameResolver::new(&[
                (first, "first.Model"),
                (second, "second.Model"),
            ]));

            assert_eq!(diagnostic.headline_message(), "Expected `second.Model`");
            assert_eq!(
                diagnostic.sub_diagnostics()[0].headline_message(),
                "Found `first.Model`"
            );
        }

        Ok(())
    }

    #[test]
    fn unwinding_removes_the_current_message_capture() {
        let result = std::panic::catch_unwind(|| {
            DiagnosticMessage::from_display(fmt::from_fn(|_| {
                panic!("diagnostic formatting failed")
            }))
        });

        assert!(result.is_err());
        MESSAGE_CAPTURES.with(|captures| assert!(captures.borrow().is_empty()));
    }

    #[test]
    fn qualifies_names_across_all_diagnostic_messages() -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.set_concise_message(format_args!("Concise `{}`", name(second, "Model")));
        diagnostic.annotate(
            Annotation::primary(Span::from(first))
                .message(format_args!("Primary `{}`", name(first, "Model"))),
        );
        diagnostic.info(format_args!("Found `{}`", name(second, "Model")));

        let mut detail = SubDiagnostic::new(
            SubDiagnosticSeverity::Info,
            format_args!("Detail `{}`", name(first, "Model")),
        );
        detail.annotate(
            Annotation::secondary(Span::from(second))
                .message(format_args!("Secondary `{}`", name(second, "Model"))),
        );
        diagnostic.sub(detail);

        let resolver = TestNameResolver::new(&[(first, "first.Model"), (second, "second.Model")]);
        diagnostic.disambiguate_names(&resolver);

        assert_eq!(diagnostic.headline_message(), "Expected `first.Model`");
        assert_eq!(diagnostic.concise_message().to_string(), "Concise `Model`");
        assert_eq!(
            diagnostic
                .primary_annotation()
                .and_then(Annotation::get_message),
            Some("Primary `first.Model`")
        );
        assert_eq!(
            diagnostic.sub_diagnostics()[0].headline_message(),
            "Found `second.Model`"
        );
        assert_eq!(
            diagnostic.sub_diagnostics()[1].headline_message(),
            "Detail `first.Model`"
        );
        assert_eq!(
            diagnostic.sub_diagnostics()[1].annotations()[0].get_message(),
            Some("Secondary `second.Model`")
        );
        assert_eq!(resolver.qualified_calls.get(), 2);
        assert_eq!(resolver.location_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn concise_messages_ignore_names_visible_only_in_notes() -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.info(format_args!("Found `{}`", name(second, "Model")));

        diagnostic.disambiguate_names(&TestNameResolver::new(&[
            (first, "first.Model"),
            (second, "second.Model"),
        ]));

        assert_eq!(diagnostic.headline_message(), "Expected `first.Model`");
        assert_eq!(diagnostic.concise_message().to_string(), "Expected `Model`");
        assert_eq!(
            diagnostic.sub_diagnostics()[0].headline_message(),
            "Found `second.Model`"
        );
        Ok(())
    }

    #[test]
    fn concise_messages_disambiguate_visible_headlines_and_annotations()
    -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.annotate(
            Annotation::primary(Span::from(second))
                .message(format_args!("Found `{}`", name(second, "Model"))),
        );

        diagnostic.disambiguate_names(&TestNameResolver::new(&[
            (first, "first.Model"),
            (second, "second.Model"),
        ]));

        assert_eq!(
            diagnostic.concise_message().to_string(),
            "Expected `first.Model`: Found `second.Model`"
        );
        Ok(())
    }

    #[test]
    fn custom_concise_messages_do_not_affect_full_diagnostics() -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.set_concise_message(format_args!("Found `{}`", name(second, "Model")));

        let resolver = TestNameResolver::new(&[(first, "first.Model"), (second, "second.Model")]);
        diagnostic.disambiguate_names(&resolver);

        assert_eq!(diagnostic.headline_message(), "Expected `Model`");
        assert_eq!(diagnostic.concise_message().to_string(), "Found `Model`");
        assert_eq!(resolver.qualified_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn custom_concise_messages_disambiguate_their_own_names() -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.set_concise_message(format_args!(
            "Expected `{}`, found `{}`",
            name(first, "Model"),
            name(second, "Model"),
        ));

        diagnostic.disambiguate_names(&TestNameResolver::new(&[
            (first, "first.Model"),
            (second, "second.Model"),
        ]));

        assert_eq!(diagnostic.headline_message(), "Expected `Model`");
        assert_eq!(
            diagnostic.concise_message().to_string(),
            "Expected `first.Model`, found `second.Model`"
        );
        Ok(())
    }

    #[test]
    fn unambiguous_names_do_not_resolve_qualified_names_or_locations() -> Result<(), Box<dyn Error>>
    {
        let (_db, first, _second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.info(format_args!("Found `{}`", name(first, "Model")));

        let resolver = TestNameResolver::new(&[(first, "first.Model")]);
        diagnostic.disambiguate_names(&resolver);

        assert_eq!(diagnostic.headline_message(), "Expected `Model`");
        assert_eq!(resolver.qualified_calls.get(), 0);
        assert_eq!(resolver.location_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn identical_qualified_names_resolve_source_locations() -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        diagnostic.info(format_args!("Found `{}`", name(second, "Model")));

        let resolver =
            TestNameResolver::new(&[(first, "package.Model"), (second, "package.Model")]);
        diagnostic.disambiguate_names(&resolver);

        assert_eq!(
            diagnostic.headline_message(),
            "Expected `package.Model @ definition`"
        );
        assert_eq!(resolver.qualified_calls.get(), 2);
        assert_eq!(resolver.location_calls.get(), 2);
        Ok(())
    }

    #[test]
    fn preserves_semantic_ranges_after_unicode_prefixes() -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Expected `{}`", name(first, "Model")),
        );
        let explanation =
            DiagnosticMessage::from_display(format_args!("élément `{}`", name(second, "Model")))
                .with_prefix("├─ ");
        diagnostic.info(explanation);

        diagnostic.disambiguate_names(&TestNameResolver::new(&[
            (first, "first.Model"),
            (second, "second.Model"),
        ]));

        assert_eq!(diagnostic.headline_message(), "Expected `first.Model`");
        assert_eq!(
            diagnostic.sub_diagnostics()[0].headline_message(),
            "├─ élément `second.Model`"
        );
        Ok(())
    }

    #[test]
    fn fixed_names_disambiguate_other_names_without_changing_representation()
    -> Result<(), Box<dyn Error>> {
        let (_db, first, second) = files()?;
        let module = fmt::from_fn(|f| {
            DiagnosticNameRecord {
                render: || f.write_str("module"),
                name: || DiagnosticName::new("module", first.as_id(), DiagnosticNameKind::Class),
            }
            .render_fixed()
        });
        let mut diagnostic = Diagnostic::new(
            DiagnosticId::InvalidSyntax,
            Severity::Error,
            format_args!("Found `<{module} 'os'>`"),
        );
        diagnostic.info(format_args!("Expected `{}`", name(second, "module")));

        diagnostic.disambiguate_names(&TestNameResolver::new(&[
            (first, "types.ModuleType"),
            (second, "custom.module"),
        ]));

        assert_eq!(diagnostic.headline_message(), "Found `<module 'os'>`");
        assert_eq!(
            diagnostic.sub_diagnostics()[0].headline_message(),
            "Expected `custom.module`"
        );
        Ok(())
    }
}
