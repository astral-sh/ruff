//! Display implementations for types.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::fmt::{self, Display, Formatter, Write};
use std::rc::Rc;

use ruff_db::files::FilePath;
use ruff_db::source::line_index;
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_literal::escape::AsciiEscape;
use ruff_text_size::{TextLen, TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::Db;
use crate::place::Place;
use crate::semantic_index::definition::Definition;
use crate::types::class::{ClassLiteral, ClassType, GenericAlias};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::signatures::{
    CallableSignature, Parameter, Parameters, ParametersKind, Signature,
};
use crate::types::tuple::TupleSpec;
use crate::types::visitor::TypeVisitor;
use crate::types::{
    BoundTypeVarIdentity, CallableType, CallableTypeKind, IntersectionType, KnownBoundMethodType,
    KnownClass, KnownInstanceType, MaterializationKind, Protocol, ProtocolInstanceType,
    SpecialFormType, StringLiteralType, SubclassOfInner, Type, TypedDictType, UnionType,
    WrapperDescriptorKind, visitor,
};

/// Settings for displaying types and signatures
#[derive(Debug, Clone, Default)]
pub struct DisplaySettings<'db> {
    /// Whether rendering can be multiline
    pub multiline: bool,
    /// Class names that should be displayed fully qualified
    /// (e.g., `module.ClassName` instead of just `ClassName`)
    pub qualified: Rc<FxHashMap<&'db str, QualificationLevel>>,
    /// Whether long unions and literals are displayed in full
    pub preserve_full_unions: bool,
    /// Disallow Signature printing to introduce a name
    /// (presumably because we rendered one already)
    pub disallow_signature_name: bool,
}

impl<'db> DisplaySettings<'db> {
    #[must_use]
    pub fn multiline(&self) -> Self {
        Self {
            multiline: true,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn singleline(&self) -> Self {
        Self {
            multiline: false,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn truncate_long_unions(self) -> Self {
        Self {
            preserve_full_unions: false,
            ..self
        }
    }

    #[must_use]
    pub fn preserve_long_unions(self) -> Self {
        Self {
            preserve_full_unions: true,
            ..self
        }
    }

    #[must_use]
    pub fn disallow_signature_name(&self) -> Self {
        Self {
            disallow_signature_name: true,
            ..self.clone()
        }
    }

    #[must_use]
    pub fn from_possibly_ambiguous_types<I, T>(db: &'db dyn Db, types: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<Type<'db>>,
    {
        fn build_display_settings<'db>(
            collector: &AmbiguousClassCollector<'db>,
        ) -> DisplaySettings<'db> {
            DisplaySettings {
                qualified: Rc::new(
                    collector
                        .class_names
                        .borrow()
                        .iter()
                        .filter_map(|(name, ambiguity)| {
                            Some((*name, QualificationLevel::from_ambiguity_state(ambiguity)?))
                        })
                        .collect(),
                ),
                ..DisplaySettings::default()
            }
        }

        let collector = AmbiguousClassCollector::default();

        for ty in types {
            collector.visit_type(db, ty.into());
        }

        build_display_settings(&collector)
    }
}

/// Details about a type's formatting
///
/// The `targets` and `details` are 1:1 (you can `zip` them)
pub struct TypeDisplayDetails<'db> {
    /// The fully formatted type
    pub label: String,
    /// Ranges in the label
    pub targets: Vec<TextRange>,
    /// Metadata for each range
    pub details: Vec<TypeDetail<'db>>,
    /// Whether the label is valid Python syntax
    pub is_valid_syntax: bool,
}

/// Abstraction over "are we doing normal formatting, or tracking ranges with metadata?"
enum TypeWriter<'a, 'b, 'db> {
    Formatter(&'a mut Formatter<'b>),
    Details(TypeDetailsWriter<'db>),
}
/// Writer that builds a string with range tracking
struct TypeDetailsWriter<'db> {
    label: String,
    targets: Vec<TextRange>,
    details: Vec<TypeDetail<'db>>,
    is_valid_syntax: bool,
}

impl<'db> TypeDetailsWriter<'db> {
    fn new() -> Self {
        Self {
            label: String::new(),
            targets: Vec::new(),
            details: Vec::new(),
            is_valid_syntax: true,
        }
    }

    /// Produce type info
    fn finish_type_details(self) -> TypeDisplayDetails<'db> {
        TypeDisplayDetails {
            label: self.label,
            targets: self.targets,
            details: self.details,
            is_valid_syntax: self.is_valid_syntax,
        }
    }

    /// Produce function signature info
    fn finish_signature_details(self) -> SignatureDisplayDetails {
        // We use SignatureStart and SignatureEnd to delimit nested function signatures inside
        // this function signature. We only care about the parameters of the outermost function
        // which should introduce it's own SignatureStart and SignatureEnd
        let mut parameter_ranges = Vec::new();
        let mut parameter_names = Vec::new();
        let mut parameter_nesting = 0;
        for (target, detail) in self.targets.into_iter().zip(self.details) {
            match detail {
                TypeDetail::SignatureStart => parameter_nesting += 1,
                TypeDetail::SignatureEnd => parameter_nesting -= 1,
                TypeDetail::Parameter(parameter) => {
                    if parameter_nesting <= 1 {
                        // We found parameters at the top-level, record them
                        parameter_names.push(parameter);
                        parameter_ranges.push(target);
                    }
                }
                TypeDetail::Type(_) => { /* don't care */ }
            }
        }

        SignatureDisplayDetails {
            label: self.label,
            parameter_names,
            parameter_ranges,
        }
    }
}

impl<'a, 'b, 'db> TypeWriter<'a, 'b, 'db> {
    /// Indicate the given detail is about to start being written to this Writer
    ///
    /// This creates a scoped guard that when Dropped will record the given detail
    /// as spanning from when it was introduced to when it was dropped.
    fn with_detail<'c>(&'c mut self, detail: TypeDetail<'db>) -> TypeDetailGuard<'a, 'b, 'c, 'db> {
        let start = match self {
            TypeWriter::Formatter(_) => None,
            TypeWriter::Details(details) => Some(details.label.text_len()),
        };
        TypeDetailGuard {
            start,
            inner: self,
            payload: Some(detail),
        }
    }

    /// Convenience for `with_detail(TypeDetail::Type(ty))`
    fn with_type<'c>(&'c mut self, ty: Type<'db>) -> TypeDetailGuard<'a, 'b, 'c, 'db> {
        self.with_detail(TypeDetail::Type(ty))
    }

    fn set_invalid_type_annotation(&mut self) {
        match self {
            TypeWriter::Formatter(_) => {}
            TypeWriter::Details(details) => details.is_valid_syntax = false,
        }
    }

    fn join<'c>(&'c mut self, separator: &'static str) -> Join<'a, 'b, 'c, 'db> {
        Join {
            fmt: self,
            separator,
            result: Ok(()),
            seen_first: false,
        }
    }
}

impl Write for TypeWriter<'_, '_, '_> {
    fn write_str(&mut self, val: &str) -> fmt::Result {
        match self {
            TypeWriter::Formatter(formatter) => formatter.write_str(val),
            TypeWriter::Details(formatter) => formatter.write_str(val),
        }
    }
}
impl Write for TypeDetailsWriter<'_> {
    fn write_str(&mut self, val: &str) -> fmt::Result {
        self.label.write_str(val)
    }
}

trait FmtDetailed<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result;
}

struct Join<'a, 'b, 'c, 'db> {
    fmt: &'c mut TypeWriter<'a, 'b, 'db>,
    separator: &'static str,
    result: fmt::Result,
    seen_first: bool,
}

impl<'db> Join<'_, '_, '_, 'db> {
    fn entry(&mut self, item: &dyn FmtDetailed<'db>) -> &mut Self {
        if self.seen_first {
            self.result = self
                .result
                .and_then(|()| self.fmt.write_str(self.separator));
        } else {
            self.seen_first = true;
        }
        self.result = self.result.and_then(|()| item.fmt_detailed(self.fmt));
        self
    }

    fn entries<I, F>(&mut self, items: I) -> &mut Self
    where
        I: IntoIterator<Item = F>,
        F: FmtDetailed<'db>,
    {
        for item in items {
            self.entry(&item);
        }
        self
    }

    fn finish(&mut self) -> fmt::Result {
        self.result
    }
}

pub enum TypeDetail<'db> {
    /// Dummy item to indicate a function signature's parameters have started
    SignatureStart,
    /// Dummy item to indicate a function signature's parameters have ended
    SignatureEnd,
    /// A function signature's parameter
    Parameter(String),
    /// A type
    Type(Type<'db>),
}

/// Look on my Works, ye Mighty, and despair!
///
/// It's quite important that we avoid conflating any of these lifetimes, or else the
/// borrowchecker will throw a ton of confusing errors about things not living long
/// enough. If you get those kinds of errors, it's probably because you introduced
/// something like `&'db self`, which, while convenient, and sometimes works, is imprecise.
struct TypeDetailGuard<'a, 'b, 'c, 'db> {
    inner: &'c mut TypeWriter<'a, 'b, 'db>,
    start: Option<TextSize>,
    payload: Option<TypeDetail<'db>>,
}

impl Drop for TypeDetailGuard<'_, '_, '_, '_> {
    fn drop(&mut self) {
        // The fallibility here is primarily retrieving `TypeWriter::Details`
        // everything else is ideally-never-fails pedantry (yay for pedantry!)
        if let TypeWriter::Details(details) = &mut self.inner
            && let Some(start) = self.start
            && let Some(payload) = self.payload.take()
        {
            let target = TextRange::new(start, details.label.text_len());
            details.targets.push(target);
            details.details.push(payload);
        }
    }
}

impl<'a, 'b, 'db> std::ops::Deref for TypeDetailGuard<'a, 'b, '_, 'db> {
    type Target = TypeWriter<'a, 'b, 'db>;
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}
impl std::ops::DerefMut for TypeDetailGuard<'_, '_, '_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualificationLevel {
    ModuleName,
    FileAndLineNumber,
}

impl QualificationLevel {
    const fn from_ambiguity_state(state: &AmbiguityState) -> Option<Self> {
        match state {
            AmbiguityState::Unambiguous(_) => None,
            AmbiguityState::RequiresFullyQualifiedName { .. } => Some(Self::ModuleName),
            AmbiguityState::RequiresFileAndLineNumber => Some(Self::FileAndLineNumber),
        }
    }
}

#[derive(Debug, Default)]
struct AmbiguousClassCollector<'db> {
    visited_types: RefCell<FxHashSet<Type<'db>>>,
    class_names: RefCell<FxHashMap<&'db str, AmbiguityState<'db>>>,
}

impl<'db> AmbiguousClassCollector<'db> {
    fn record_class(&self, db: &'db dyn Db, class: ClassLiteral<'db>) {
        match self.class_names.borrow_mut().entry(class.name(db)) {
            Entry::Vacant(entry) => {
                entry.insert(AmbiguityState::Unambiguous(class));
            }
            Entry::Occupied(mut entry) => {
                let value = entry.get_mut();
                match value {
                    AmbiguityState::Unambiguous(existing) => {
                        if *existing != class {
                            let qualified_name_components =
                                class.qualified_name(db).components_excluding_self();
                            if existing.qualified_name(db).components_excluding_self()
                                == qualified_name_components
                            {
                                *value = AmbiguityState::RequiresFileAndLineNumber;
                            } else {
                                *value = AmbiguityState::RequiresFullyQualifiedName {
                                    class,
                                    qualified_name_components,
                                };
                            }
                        }
                    }
                    AmbiguityState::RequiresFullyQualifiedName {
                        class: existing,
                        qualified_name_components,
                    } => {
                        if *existing != class {
                            let new_components =
                                class.qualified_name(db).components_excluding_self();
                            if *qualified_name_components == new_components {
                                *value = AmbiguityState::RequiresFileAndLineNumber;
                            }
                        }
                    }
                    AmbiguityState::RequiresFileAndLineNumber => {}
                }
            }
        }
    }
}

/// Whether or not a class can be unambiguously identified by its *unqualified* name
/// given the other types that are present in the same context.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AmbiguityState<'db> {
    /// The class can be displayed unambiguously using its unqualified name
    Unambiguous(ClassLiteral<'db>),
    /// The class must be displayed using its fully qualified name to avoid ambiguity.
    RequiresFullyQualifiedName {
        class: ClassLiteral<'db>,
        qualified_name_components: Vec<String>,
    },
    /// Even the class's fully qualified name is not sufficient;
    /// we must also include the file and line number.
    RequiresFileAndLineNumber,
}

impl<'db> TypeVisitor<'db> for AmbiguousClassCollector<'db> {
    fn should_visit_lazy_type_attributes(&self) -> bool {
        false
    }

    fn visit_type(&self, db: &'db dyn Db, ty: Type<'db>) {
        match ty {
            Type::ClassLiteral(class) => self.record_class(db, class),
            Type::EnumLiteral(literal) => self.record_class(db, literal.enum_class(db)),
            Type::GenericAlias(alias) => self.record_class(db, alias.origin(db)),
            // Visit the class (as if it were a nominal-instance type)
            // rather than the protocol members, if it is a class-based protocol.
            // (For the purposes of displaying the type, we'll use the class name.)
            Type::ProtocolInstance(ProtocolInstanceType {
                inner: Protocol::FromClass(class),
                ..
            }) => return self.visit_type(db, Type::from(class)),
            // no need to recurse into TypeVar bounds/constraints
            Type::TypeVar(_) => return,
            _ => {}
        }

        if let visitor::TypeKind::NonAtomic(t) = visitor::TypeKind::from(ty) {
            if !self.visited_types.borrow_mut().insert(ty) {
                // If we have already seen this type, we can skip it.
                return;
            }
            visitor::walk_non_atomic_type(db, t, self);
        }
    }
}

impl<'db> Type<'db> {
    pub fn display(self, db: &'db dyn Db) -> DisplayType<'db> {
        DisplayType {
            ty: self,
            settings: DisplaySettings::from_possibly_ambiguous_types(db, [self]),
            db,
        }
    }

    pub fn display_with(self, db: &'db dyn Db, settings: DisplaySettings<'db>) -> DisplayType<'db> {
        DisplayType {
            ty: self,
            db,
            settings,
        }
    }

    fn representation(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayRepresentation<'db> {
        DisplayRepresentation {
            db,
            ty: self,
            settings,
        }
    }
}

pub struct DisplayType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> DisplayType<'db> {
    pub fn to_string_parts(&self) -> TypeDisplayDetails<'db> {
        let mut f = TypeWriter::Details(TypeDetailsWriter::new());
        self.fmt_detailed(&mut f).unwrap();

        match f {
            TypeWriter::Details(details) => details.finish_type_details(),
            TypeWriter::Formatter(_) => unreachable!("Expected Details variant"),
        }
    }
}

impl<'db> FmtDetailed<'db> for DisplayType<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let representation = self.ty.representation(self.db, self.settings.clone());
        match self.ty {
            Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_) => {
                f.with_type(Type::SpecialForm(SpecialFormType::Literal))
                    .write_str("Literal")?;
                f.write_char('[')?;
                representation.fmt_detailed(f)?;
                f.write_str("]")
            }
            _ => representation.fmt_detailed(f),
        }
    }
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl fmt::Debug for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl<'db> ClassLiteral<'db> {
    fn display_with(self, db: &'db dyn Db, settings: DisplaySettings<'db>) -> ClassDisplay<'db> {
        ClassDisplay {
            db,
            class: self,
            settings,
        }
    }
}

struct ClassDisplay<'db> {
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for ClassDisplay<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let qualification_level = self.settings.qualified.get(&**self.class.name(self.db));

        let ty = Type::ClassLiteral(self.class);
        if qualification_level.is_some() {
            write!(f.with_type(ty), "{}", self.class.qualified_name(self.db))?;
        } else {
            write!(f.with_type(ty), "{}", self.class.name(self.db))?;
        }

        if qualification_level == Some(&QualificationLevel::FileAndLineNumber) {
            let file = self.class.file(self.db);
            let path = file.path(self.db);
            let path = match path {
                FilePath::System(path) => Cow::Owned(FilePath::System(
                    path.strip_prefix(self.db.system().current_directory())
                        .unwrap_or(path)
                        .to_path_buf(),
                )),
                FilePath::Vendored(_) | FilePath::SystemVirtual(_) => Cow::Borrowed(path),
            };
            let line_index = line_index(self.db, file);
            let class_offset = self.class.header_range(self.db).start();
            let line_number = line_index.line_index(class_offset);
            f.set_invalid_type_annotation();
            write!(f, " @ {path}:{line_number}")?;
        }
        Ok(())
    }
}

impl Display for ClassDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

/// Writes the string representation of a type, which is the value displayed either as
/// `Literal[<repr>]` or `Literal[<repr1>, <repr2>]` for literal types or as `<repr>` for
/// non literals
struct DisplayRepresentation<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayRepresentation<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> FmtDetailed<'db> for DisplayRepresentation<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        match self.ty {
            Type::Dynamic(dynamic) => {
                if dynamic.is_todo() {
                    f.set_invalid_type_annotation();
                }
                write!(f.with_type(self.ty), "{dynamic}")
            }
            Type::Never => f.with_type(self.ty).write_str("Never"),
            Type::NominalInstance(instance) => {
                let class = instance.class(self.db);

                match (class, class.known(self.db)) {
                    (_, Some(KnownClass::NoneType)) => f.with_type(self.ty).write_str("None"),
                    (_, Some(KnownClass::NoDefaultType)) => f.with_type(self.ty).write_str("NoDefault"),
                    (ClassType::Generic(alias), Some(KnownClass::Tuple)) => alias
                        .specialization(self.db)
                        .tuple(self.db)
                        .expect("Specialization::tuple() should always return `Some()` for `KnownClass::Tuple`")
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f),
                    (ClassType::NonGeneric(class), _) => {
                        class.display_with(self.db, self.settings.clone()).fmt_detailed(f)
                    },
                    (ClassType::Generic(alias), _) => alias.display_with(self.db, self.settings.clone()).fmt_detailed(f),
                }
            }
            Type::ProtocolInstance(protocol) => match protocol.inner {
                Protocol::FromClass(class) => match *class {
                    ClassType::NonGeneric(class) => class
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f),
                    ClassType::Generic(alias) => alias
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f),
                },
                Protocol::Synthesized(synthetic) => {
                    f.set_invalid_type_annotation();
                    f.write_char('<')?;
                    f.with_type(Type::SpecialForm(SpecialFormType::Protocol))
                        .write_str("Protocol")?;
                    f.write_str(" with members ")?;
                    let interface = synthetic.interface();
                    let member_list = interface.members(self.db);
                    let num_members = member_list.len();
                    for (i, member) in member_list.enumerate() {
                        let is_last = i == num_members - 1;
                        write!(f, "'{}'", member.name())?;
                        if !is_last {
                            f.write_str(", ")?;
                        }
                    }
                    f.write_char('>')
                }
            },
            Type::PropertyInstance(_) => f.with_type(self.ty).write_str("property"),
            Type::ModuleLiteral(module) => {
                f.set_invalid_type_annotation();
                f.write_char('<')?;
                f.with_type(KnownClass::ModuleType.to_class_literal(self.db))
                    .write_str("module")?;
                f.write_str(" '")?;
                f.with_type(self.ty)
                    .write_str(module.module(self.db).name(self.db))?;
                f.write_str("'>")
            }
            Type::ClassLiteral(class) => {
                f.set_invalid_type_annotation();
                let mut f = f.with_type(self.ty);
                f.write_str("<class '")?;
                class
                    .display_with(self.db, self.settings.clone())
                    .fmt_detailed(&mut f)?;
                f.write_str("'>")
            }
            Type::GenericAlias(generic) => {
                f.set_invalid_type_annotation();
                let mut f = f.with_type(self.ty);
                f.write_str("<class '")?;
                generic
                    .display_with(self.db, self.settings.clone())
                    .fmt_detailed(&mut f)?;
                f.write_str("'>")
            }
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(ClassType::NonGeneric(class)) => {
                    f.with_type(KnownClass::Type.to_class_literal(self.db))
                        .write_str("type")?;
                    f.write_char('[')?;
                    class
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f)?;
                    f.write_char(']')
                }
                SubclassOfInner::Class(ClassType::Generic(alias)) => {
                    f.with_type(KnownClass::Type.to_class_literal(self.db))
                        .write_str("type")?;
                    f.write_char('[')?;
                    alias
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f)?;
                    f.write_char(']')
                }
                SubclassOfInner::Dynamic(dynamic) => {
                    f.with_type(KnownClass::Type.to_class_literal(self.db))
                        .write_str("type")?;
                    f.write_char('[')?;
                    write!(f.with_type(Type::Dynamic(dynamic)), "{dynamic}")?;
                    f.write_char(']')
                }
                SubclassOfInner::TypeVar(bound_typevar) => {
                    f.set_invalid_type_annotation();
                    f.with_type(KnownClass::Type.to_class_literal(self.db))
                        .write_str("type")?;
                    f.write_char('[')?;
                    write!(
                        f.with_type(Type::TypeVar(bound_typevar)),
                        "{}",
                        bound_typevar.identity(self.db).display(self.db)
                    )?;
                    f.write_char(']')
                }
            },
            Type::SpecialForm(special_form) => {
                f.set_invalid_type_annotation();
                write!(f.with_type(self.ty), "<special-form '{special_form}'>")
            }
            Type::KnownInstance(known_instance) => known_instance
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f),
            Type::FunctionLiteral(function) => function
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f),
            Type::Callable(callable) => callable
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f),
            Type::BoundMethod(bound_method) => {
                let function = bound_method.function(self.db);
                let self_ty = bound_method.self_instance(self.db);
                let typing_self_ty = bound_method.typing_self_type(self.db);

                match function.signature(self.db).overloads.as_slice() {
                    [signature] => {
                        let type_parameters = DisplayOptionalGenericContext {
                            generic_context: signature.generic_context.as_ref(),
                            db: self.db,
                            settings: self.settings.clone(),
                        };
                        f.set_invalid_type_annotation();
                        f.write_str("bound method ")?;
                        self_ty
                            .display_with(self.db, self.settings.singleline())
                            .fmt_detailed(f)?;
                        f.write_char('.')?;
                        f.with_type(self.ty).write_str(function.name(self.db))?;
                        type_parameters.fmt_detailed(f)?;
                        signature
                            .bind_self(self.db, Some(typing_self_ty))
                            .display_with(self.db, self.settings.disallow_signature_name())
                            .fmt_detailed(f)
                    }
                    signatures => {
                        // TODO: How to display overloads?
                        if !self.settings.multiline {
                            // TODO: This should ideally have a TypeDetail but we actually
                            // don't have a type for @overload (we just detect the decorator)
                            f.write_str("Overload")?;
                            f.write_char('[')?;
                        }
                        let separator = if self.settings.multiline { "\n" } else { ", " };
                        let mut join = f.join(separator);
                        for signature in signatures {
                            join.entry(
                                &signature
                                    .bind_self(self.db, Some(typing_self_ty))
                                    .display_with(self.db, self.settings.clone()),
                            );
                        }
                        join.finish()?;
                        if !self.settings.multiline {
                            f.write_str("]")?;
                        }
                        Ok(())
                    }
                }
            }
            Type::KnownBoundMethod(method_type) => {
                f.set_invalid_type_annotation();
                let (cls, member_name, cls_name, ty, ty_name) = match method_type {
                    KnownBoundMethodType::FunctionTypeDunderGet(function) => (
                        KnownClass::FunctionType,
                        "__get__",
                        "function",
                        Type::FunctionLiteral(function),
                        Some(&**function.name(self.db)),
                    ),
                    KnownBoundMethodType::FunctionTypeDunderCall(function) => (
                        KnownClass::FunctionType,
                        "__call__",
                        "function",
                        Type::FunctionLiteral(function),
                        Some(&**function.name(self.db)),
                    ),
                    KnownBoundMethodType::PropertyDunderGet(property) => (
                        KnownClass::Property,
                        "__get__",
                        "property",
                        Type::PropertyInstance(property),
                        property
                            .getter(self.db)
                            .and_then(Type::as_function_literal)
                            .map(|getter| &**getter.name(self.db)),
                    ),
                    KnownBoundMethodType::PropertyDunderSet(property) => (
                        KnownClass::Property,
                        "__set__",
                        "property",
                        Type::PropertyInstance(property),
                        property
                            .getter(self.db)
                            .and_then(Type::as_function_literal)
                            .map(|getter| &**getter.name(self.db)),
                    ),
                    KnownBoundMethodType::StrStartswith(literal) => (
                        KnownClass::Property,
                        "startswith",
                        "string",
                        Type::StringLiteral(literal),
                        Some(literal.value(self.db)),
                    ),
                    KnownBoundMethodType::ConstraintSetRange => {
                        return f.write_str("bound method `ConstraintSet.range`");
                    }
                    KnownBoundMethodType::ConstraintSetAlways => {
                        return f.write_str("bound method `ConstraintSet.always`");
                    }
                    KnownBoundMethodType::ConstraintSetNever => {
                        return f.write_str("bound method `ConstraintSet.never`");
                    }
                    KnownBoundMethodType::ConstraintSetImpliesSubtypeOf(_) => {
                        return f.write_str("bound method `ConstraintSet.implies_subtype_of`");
                    }
                    KnownBoundMethodType::ConstraintSetSatisfies(_) => {
                        return f.write_str("bound method `ConstraintSet.satisfies`");
                    }
                    KnownBoundMethodType::ConstraintSetSatisfiedByAllTypeVars(_) => {
                        return f
                            .write_str("bound method `ConstraintSet.satisfied_by_all_typevars`");
                    }
                    KnownBoundMethodType::GenericContextSpecializeConstrained(_) => {
                        return f.write_str("bound method `GenericContext.specialize_constrained`");
                    }
                };

                let class_ty = cls.to_class_literal(self.db);
                f.write_char('<')?;
                f.with_type(KnownClass::MethodWrapperType.to_class_literal(self.db))
                    .write_str("method-wrapper")?;
                f.write_str(" '")?;
                if let Place::Defined(member_ty, _, _, _) =
                    class_ty.member(self.db, member_name).place
                {
                    f.with_type(member_ty).write_str(member_name)?;
                } else {
                    f.write_str(member_name)?;
                }
                f.write_str("' of ")?;
                f.with_type(class_ty).write_str(cls_name)?;
                if let Some(name) = ty_name {
                    f.write_str(" '")?;
                    f.with_type(ty).write_str(name)?;
                    f.write_str("'>")
                } else {
                    f.write_str("' object>")
                }
            }
            Type::WrapperDescriptor(kind) => {
                f.set_invalid_type_annotation();
                let (method, object, cls) = match kind {
                    WrapperDescriptorKind::FunctionTypeDunderGet => {
                        ("__get__", "function", KnownClass::FunctionType)
                    }
                    WrapperDescriptorKind::PropertyDunderGet => {
                        ("__get__", "property", KnownClass::Property)
                    }
                    WrapperDescriptorKind::PropertyDunderSet => {
                        ("__set__", "property", KnownClass::Property)
                    }
                };
                f.write_char('<')?;
                f.with_type(KnownClass::WrapperDescriptorType.to_class_literal(self.db))
                    .write_str("wrapper-descriptor")?;
                f.write_str(" '")?;
                f.write_str(method)?;
                f.write_str("' of '")?;
                f.with_type(cls.to_class_literal(self.db))
                    .write_str(object)?;
                f.write_str("' objects>")
            }
            Type::DataclassDecorator(_) => {
                f.set_invalid_type_annotation();
                f.write_str("<decorator produced by dataclass-like function>")
            }
            Type::DataclassTransformer(_) => {
                f.set_invalid_type_annotation();
                f.write_str("<decorator produced by typing.dataclass_transform>")
            }
            Type::Union(union) => union
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f),
            Type::Intersection(intersection) => intersection
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f),
            Type::IntLiteral(n) => write!(f.with_type(self.ty), "{n}"),
            Type::BooleanLiteral(boolean) => {
                f.with_type(self.ty)
                    .write_str(if boolean { "True" } else { "False" })
            }
            Type::StringLiteral(string) => {
                write!(
                    f.with_type(self.ty),
                    "{}",
                    string.display_with(self.db, self.settings.clone())
                )
            }
            // an alternative would be to use `Type::SpecialForm(SpecialFormType::LiteralString)` here,
            // which would mean users would be able to jump to the definition of `LiteralString` from the
            // inlay hint, but that seems less useful than the definition of `str` for a variable that is
            // inferred as an *inhabitant* of `LiteralString` (since that variable will just be a string
            // at runtime)
            Type::LiteralString => f.with_type(self.ty).write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape = AsciiEscape::with_preferred_quote(bytes.value(self.db), Quote::Double);

                write!(
                    f.with_type(self.ty),
                    "{}",
                    escape.bytes_repr(TripleQuotes::No)
                )
            }
            Type::EnumLiteral(enum_literal) => {
                enum_literal
                    .enum_class(self.db)
                    .display_with(self.db, self.settings.clone())
                    .fmt_detailed(f)?;
                write!(f, ".{}", enum_literal.name(self.db))
            }
            Type::TypeVar(bound_typevar) => {
                f.set_invalid_type_annotation();
                write!(f, "{}", bound_typevar.identity(self.db).display(self.db))
            }
            Type::AlwaysTruthy => f.with_type(self.ty).write_str("AlwaysTruthy"),
            Type::AlwaysFalsy => f.with_type(self.ty).write_str("AlwaysFalsy"),
            Type::BoundSuper(bound_super) => {
                f.set_invalid_type_annotation();
                f.write_str("<super: ")?;
                Type::from(bound_super.pivot_class(self.db))
                    .display_with(self.db, self.settings.singleline())
                    .fmt_detailed(f)?;
                f.write_str(", ")?;
                Type::from(bound_super.owner(self.db))
                    .display_with(self.db, self.settings.singleline())
                    .fmt_detailed(f)?;
                f.write_str(">")
            }
            Type::TypeIs(type_is) => {
                f.with_type(Type::SpecialForm(SpecialFormType::TypeIs))
                    .write_str("TypeIs")?;
                f.write_char('[')?;
                type_is
                    .return_type(self.db)
                    .display_with(self.db, self.settings.singleline())
                    .fmt_detailed(f)?;
                if let Some(name) = type_is.place_name(self.db) {
                    f.set_invalid_type_annotation();
                    f.write_str(" @ ")?;
                    f.write_str(&name)?;
                }
                f.write_str("]")
            }
            Type::TypedDict(TypedDictType::Class(defining_class)) => match defining_class {
                ClassType::NonGeneric(class) => class
                    .display_with(self.db, self.settings.clone())
                    .fmt_detailed(f),
                ClassType::Generic(alias) => alias
                    .display_with(self.db, self.settings.clone())
                    .fmt_detailed(f),
            },
            Type::TypedDict(TypedDictType::Synthesized(synthesized)) => {
                f.set_invalid_type_annotation();
                f.write_char('<')?;
                f.with_type(Type::SpecialForm(SpecialFormType::TypedDict))
                    .write_str("TypedDict")?;
                f.write_str(" with items ")?;
                let items = synthesized.items(self.db);
                for (i, name) in items.keys().enumerate() {
                    let is_last = i == items.len() - 1;
                    write!(f, "'{name}'")?;
                    if !is_last {
                        f.write_str(", ")?;
                    }
                }
                f.write_char('>')
            }
            Type::TypeAlias(alias) => {
                f.write_str(alias.name(self.db))?;
                match alias.specialization(self.db) {
                    None => Ok(()),
                    Some(specialization) => specialization
                        .display_short(self.db, TupleSpecialization::No, self.settings.clone())
                        .fmt_detailed(f),
                }
            }
            Type::NewTypeInstance(newtype) => f.with_type(self.ty).write_str(newtype.name(self.db)),
        }
    }
}

impl<'db> BoundTypeVarIdentity<'db> {
    pub(crate) fn display(self, db: &'db dyn Db) -> impl Display {
        DisplayBoundTypeVarIdentity {
            bound_typevar_identity: self,
            db,
        }
    }
}

struct DisplayBoundTypeVarIdentity<'db> {
    bound_typevar_identity: BoundTypeVarIdentity<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayBoundTypeVarIdentity<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.bound_typevar_identity.identity.name(self.db))?;
        if let Some(binding_context) = self.bound_typevar_identity.binding_context.name(self.db) {
            write!(f, "@{binding_context}")?;
        }
        if let Some(paramspec_attr) = self.bound_typevar_identity.paramspec_attr {
            write!(f, ".{paramspec_attr}")?;
        }
        Ok(())
    }
}

impl<'db> TupleSpec<'db> {
    pub(crate) fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayTuple<'a, 'db> {
        DisplayTuple {
            tuple: self,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayTuple<'a, 'db> {
    tuple: &'a TupleSpec<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayTuple<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        f.with_type(KnownClass::Tuple.to_class_literal(self.db))
            .write_str("tuple")?;
        f.write_char('[')?;
        match self.tuple {
            TupleSpec::Fixed(tuple) => {
                let elements = tuple.elements_slice();
                if elements.is_empty() {
                    f.write_str("()")?;
                } else {
                    elements
                        .display_with(self.db, self.settings.singleline())
                        .fmt_detailed(f)?;
                }
            }

            // Decoder key for which snippets of text need to be included depending on whether
            // the tuple contains a prefix and/or suffix:
            //
            // tuple[            yyy, ...      ]
            // tuple[xxx, *tuple[yyy, ...]     ]
            // tuple[xxx, *tuple[yyy, ...], zzz]
            // tuple[     *tuple[yyy, ...], zzz]
            //       PPPPPPPPPPPP        P
            //            SSSSSSS        SSSSSS
            //
            // (Anything that appears above only a P is included only if there's a prefix; anything
            // above only an S is included only if there's a suffix; anything about both a P and an
            // S is included if there is either a prefix or a suffix. The initial `tuple[` and
            // trailing `]` are printed elsewhere. The `yyy, ...` is printed no matter what.)
            TupleSpec::Variable(tuple) => {
                if !tuple.prefix_elements().is_empty() {
                    tuple
                        .prefix_elements()
                        .display_with(self.db, self.settings.singleline())
                        .fmt_detailed(f)?;
                    f.write_str(", ")?;
                }
                if !tuple.prefix_elements().is_empty() || !tuple.suffix_elements().is_empty() {
                    f.write_char('*')?;
                    // Might as well link the type again here too
                    f.with_type(KnownClass::Tuple.to_class_literal(self.db))
                        .write_str("tuple")?;
                    f.write_char('[')?;
                }
                tuple
                    .variable()
                    .display_with(self.db, self.settings.singleline())
                    .fmt_detailed(f)?;
                f.write_str(", ...")?;
                if !tuple.prefix_elements().is_empty() || !tuple.suffix_elements().is_empty() {
                    f.write_str("]")?;
                }
                if !tuple.suffix_elements().is_empty() {
                    f.write_str(", ")?;
                    tuple
                        .suffix_elements()
                        .display_with(self.db, self.settings.singleline())
                        .fmt_detailed(f)?;
                }
            }
        }
        f.write_str("]")
    }
}

impl Display for DisplayTuple<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> OverloadLiteral<'db> {
    // Not currently used, but useful for debugging.
    #[expect(dead_code)]
    pub(crate) fn display(self, db: &'db dyn Db) -> DisplayOverloadLiteral<'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }

    pub(crate) fn display_with(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayOverloadLiteral<'db> {
        DisplayOverloadLiteral {
            literal: self,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayOverloadLiteral<'db> {
    literal: OverloadLiteral<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayOverloadLiteral<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let signature = self.literal.signature(self.db);
        let type_parameters = DisplayOptionalGenericContext {
            generic_context: signature.generic_context.as_ref(),
            db: self.db,
            settings: self.settings.clone(),
        };

        f.set_invalid_type_annotation();
        f.write_str("def ")?;
        write!(f, "{}", self.literal.name(self.db))?;
        type_parameters.fmt_detailed(f)?;
        signature
            .display_with(self.db, self.settings.disallow_signature_name())
            .fmt_detailed(f)
    }
}

impl Display for DisplayOverloadLiteral<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> FunctionType<'db> {
    pub(crate) fn display_with(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayFunctionType<'db> {
        DisplayFunctionType {
            ty: self,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayFunctionType<'db> {
    ty: FunctionType<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayFunctionType<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let signature = self.ty.signature(self.db);

        match signature.overloads.as_slice() {
            [signature] => {
                let type_parameters = DisplayOptionalGenericContext {
                    generic_context: signature.generic_context.as_ref(),
                    db: self.db,
                    settings: self.settings.clone(),
                };
                f.set_invalid_type_annotation();
                f.write_str("def ")?;
                write!(f, "{}", self.ty.name(self.db))?;
                type_parameters.fmt_detailed(f)?;
                signature
                    .display_with(self.db, self.settings.disallow_signature_name())
                    .fmt_detailed(f)
            }
            signatures => {
                // TODO: How to display overloads?
                if !self.settings.multiline {
                    // TODO: This should ideally have a TypeDetail but we actually
                    // don't have a type for @overload (we just detect the decorator)
                    f.write_str("Overload")?;
                    f.write_char('[')?;
                }
                let separator = if self.settings.multiline { "\n" } else { ", " };
                let mut join = f.join(separator);
                for signature in signatures {
                    join.entry(&signature.display_with(self.db, self.settings.clone()));
                }
                join.finish()?;
                if !self.settings.multiline {
                    f.write_str("]")?;
                }
                Ok(())
            }
        }
    }
}

impl Display for DisplayFunctionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> GenericAlias<'db> {
    pub(crate) fn display(self, db: &'db dyn Db) -> DisplayGenericAlias<'db> {
        self.display_with(db, DisplaySettings::default())
    }

    pub(crate) fn display_with(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayGenericAlias<'db> {
        DisplayGenericAlias {
            origin: self.origin(db),
            specialization: self.specialization(db),
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayGenericAlias<'db> {
    origin: ClassLiteral<'db>,
    specialization: Specialization<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayGenericAlias<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        if let Some(tuple) = self.specialization.tuple(self.db) {
            tuple
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f)
        } else {
            let prefix_details = match self.specialization.materialization_kind(self.db) {
                None => None,
                Some(MaterializationKind::Top) => Some(("Top", SpecialFormType::Top)),
                Some(MaterializationKind::Bottom) => Some(("Bottom", SpecialFormType::Bottom)),
            };
            let suffix = match self.specialization.materialization_kind(self.db) {
                None => "",
                Some(_) => "]",
            };
            if let Some((name, form)) = prefix_details {
                f.with_type(Type::SpecialForm(form)).write_str(name)?;
                f.write_char('[')?;
            }
            self.origin
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f)?;
            self.specialization
                .display_short(
                    self.db,
                    TupleSpecialization::from_class(self.db, self.origin),
                    self.settings.clone(),
                )
                .fmt_detailed(f)?;
            f.write_str(suffix)
        }
    }
}

impl Display for DisplayGenericAlias<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> GenericContext<'db> {
    pub fn display<'a>(&'a self, db: &'db dyn Db) -> DisplayGenericContext<'a, 'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }

    pub fn display_full<'a>(&'a self, db: &'db dyn Db) -> DisplayGenericContext<'a, 'db> {
        DisplayGenericContext {
            generic_context: self,
            db,
            settings: DisplaySettings::default(),
            full: true,
        }
    }

    pub fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayGenericContext<'a, 'db> {
        DisplayGenericContext {
            generic_context: self,
            db,
            settings,
            full: false,
        }
    }
}

struct DisplayOptionalGenericContext<'a, 'db> {
    generic_context: Option<&'a GenericContext<'db>>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayOptionalGenericContext<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        if let Some(generic_context) = self.generic_context {
            generic_context
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f)
        } else {
            Ok(())
        }
    }
}

impl Display for DisplayOptionalGenericContext<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

pub struct DisplayGenericContext<'a, 'db> {
    generic_context: &'a GenericContext<'db>,
    db: &'db dyn Db,
    #[expect(dead_code)]
    settings: DisplaySettings<'db>,
    full: bool,
}

impl<'db> DisplayGenericContext<'_, 'db> {
    fn fmt_normal(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let variables = self.generic_context.variables(self.db);

        let non_implicit_variables: Vec<_> = variables
            .filter(|bound_typevar| !bound_typevar.typevar(self.db).is_self(self.db))
            .collect();

        if non_implicit_variables.is_empty() {
            return Ok(());
        }

        f.write_char('[')?;
        for (idx, bound_typevar) in non_implicit_variables.iter().enumerate() {
            if idx > 0 {
                f.write_str(", ")?;
            }
            f.set_invalid_type_annotation();
            let typevar = bound_typevar.typevar(self.db);
            if typevar.is_paramspec(self.db) {
                f.write_str("**")?;
            }
            write!(
                f.with_type(Type::TypeVar(*bound_typevar)),
                "{}",
                typevar.name(self.db)
            )?;
        }
        f.write_char(']')
    }

    fn fmt_full(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let variables = self.generic_context.variables(self.db);
        f.write_char('[')?;
        for (idx, bound_typevar) in variables.enumerate() {
            if idx > 0 {
                f.write_str(", ")?;
            }
            f.set_invalid_type_annotation();
            write!(
                f.with_type(Type::TypeVar(bound_typevar)),
                "{}",
                bound_typevar.identity(self.db).display(self.db)
            )?;
        }
        f.write_char(']')
    }
}

impl<'db> FmtDetailed<'db> for DisplayGenericContext<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        if self.full {
            self.fmt_full(f)
        } else {
            self.fmt_normal(f)
        }
    }
}

impl Display for DisplayGenericContext<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> Specialization<'db> {
    pub fn display(self, db: &'db dyn Db) -> DisplaySpecialization<'db> {
        self.display_short(db, TupleSpecialization::No, DisplaySettings::default())
    }

    pub(crate) fn display_full(self, db: &'db dyn Db) -> DisplaySpecialization<'db> {
        DisplaySpecialization {
            specialization: self,
            db,
            tuple_specialization: TupleSpecialization::No,
            settings: DisplaySettings::default(),
            full: true,
        }
    }

    /// Renders the specialization as it would appear in a subscript expression, e.g. `[int, str]`.
    pub fn display_short(
        self,
        db: &'db dyn Db,
        tuple_specialization: TupleSpecialization,
        settings: DisplaySettings<'db>,
    ) -> DisplaySpecialization<'db> {
        DisplaySpecialization {
            specialization: self,
            db,
            tuple_specialization,
            settings,
            full: false,
        }
    }
}

pub struct DisplaySpecialization<'db> {
    specialization: Specialization<'db>,
    db: &'db dyn Db,
    tuple_specialization: TupleSpecialization,
    settings: DisplaySettings<'db>,
    full: bool,
}

impl<'db> DisplaySpecialization<'db> {
    fn fmt_normal(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        f.write_char('[')?;
        let types = self.specialization.types(self.db);
        for (idx, ty) in types.iter().enumerate() {
            if idx > 0 {
                f.write_str(", ")?;
            }
            ty.display_with(self.db, self.settings.clone())
                .fmt_detailed(f)?;
        }
        if self.tuple_specialization.is_yes() {
            f.write_str(", ...")?;
        }
        f.write_char(']')
    }

    fn fmt_full(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        f.write_char('[')?;
        let variables = self
            .specialization
            .generic_context(self.db)
            .variables(self.db);
        let types = self.specialization.types(self.db);
        for (idx, (bound_typevar, ty)) in variables.zip(types).enumerate() {
            if idx > 0 {
                f.write_str(", ")?;
            }
            f.set_invalid_type_annotation();
            write!(f, "{}", bound_typevar.identity(self.db).display(self.db))?;
            f.write_str(" = ")?;
            ty.display_with(self.db, self.settings.clone())
                .fmt_detailed(f)?;
        }
        f.write_char(']')
    }
}

impl<'db> FmtDetailed<'db> for DisplaySpecialization<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        if self.full {
            self.fmt_full(f)
        } else {
            self.fmt_normal(f)
        }
    }
}

impl Display for DisplaySpecialization<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TupleSpecialization {
    Yes,
    No,
}

impl TupleSpecialization {
    const fn is_yes(self) -> bool {
        matches!(self, Self::Yes)
    }

    fn from_class(db: &dyn Db, class: ClassLiteral) -> Self {
        if class.is_tuple(db) {
            Self::Yes
        } else {
            Self::No
        }
    }
}

impl<'db> CallableType<'db> {
    pub(crate) fn display<'a>(&'a self, db: &'db dyn Db) -> DisplayCallableType<'a, 'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }

    pub(crate) fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayCallableType<'a, 'db> {
        DisplayCallableType {
            signatures: self.signatures(db),
            kind: self.kind(db),
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayCallableType<'a, 'db> {
    signatures: &'a CallableSignature<'db>,
    kind: CallableTypeKind,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayCallableType<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        match self.signatures.overloads.as_slice() {
            [signature] => {
                if matches!(self.kind, CallableTypeKind::ParamSpecValue) {
                    if signature.parameters().is_top() {
                        f.write_str("Top[")?;
                    }
                    signature
                        .parameters()
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f)?;
                    if signature.parameters().is_top() {
                        f.write_str("]")?;
                    }
                } else {
                    signature
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f)?;
                }
            }
            signatures => {
                // TODO: How to display overloads?
                if !self.settings.multiline {
                    // TODO: This should ideally have a TypeDetail but we actually
                    // don't have a type for @overload (we just detect the decorator)
                    f.write_str("Overload")?;
                    f.write_char('[')?;
                }
                let separator = if self.settings.multiline { "\n" } else { ", " };
                let mut join = f.join(separator);
                for signature in signatures {
                    join.entry(&signature.display_with(self.db, self.settings.clone()));
                }
                join.finish()?;
                if !self.settings.multiline {
                    f.write_char(']')?;
                }
            }
        }

        Ok(())
    }
}

impl Display for DisplayCallableType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> Signature<'db> {
    pub(crate) fn display<'a>(&'a self, db: &'db dyn Db) -> DisplaySignature<'a, 'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }

    pub(crate) fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplaySignature<'a, 'db> {
        DisplaySignature {
            definition: self.definition(),
            parameters: self.parameters(),
            return_ty: self.return_ty,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplaySignature<'a, 'db> {
    definition: Option<Definition<'db>>,
    parameters: &'a Parameters<'db>,
    return_ty: Option<Type<'db>>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl DisplaySignature<'_, '_> {
    /// Get detailed display information including component ranges
    pub(crate) fn to_string_parts(&self) -> SignatureDisplayDetails {
        let mut f = TypeWriter::Details(TypeDetailsWriter::new());
        self.fmt_detailed(&mut f).unwrap();

        match f {
            TypeWriter::Details(details) => details.finish_signature_details(),
            TypeWriter::Formatter(_) => unreachable!("Expected Details variant"),
        }
    }
}

impl<'db> FmtDetailed<'db> for DisplaySignature<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        // Immediately write a marker signaling we're starting a signature
        let _ = f.with_detail(TypeDetail::SignatureStart);
        f.set_invalid_type_annotation();
        // When we exit this function, write a marker signaling we're ending a signature
        let mut f = f.with_detail(TypeDetail::SignatureEnd);

        if self.parameters.is_top() {
            f.write_str("Top[")?;
        }

        // If we're multiline printing and a name hasn't been emitted, try to
        // remember what the name was by checking if we have a definition
        if self.settings.multiline
            && !self.settings.disallow_signature_name
            && let Some(definition) = self.definition
            && let Some(name) = definition.name(self.db)
        {
            f.write_str("def ")?;
            f.write_str(&name)?;
        }

        // Parameters
        self.parameters
            .display_with(self.db, self.settings.clone())
            .fmt_detailed(&mut f)?;

        // Return type
        let return_ty = self.return_ty.unwrap_or_else(Type::unknown);
        f.write_str(" -> ")?;
        return_ty
            .display_with(self.db, self.settings.singleline())
            .fmt_detailed(&mut f)?;

        if self.parameters.is_top() {
            f.write_str("]")?;
        }

        Ok(())
    }
}

impl Display for DisplaySignature<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

/// Details about signature display components, including ranges for parameters and return type
#[derive(Debug, Clone)]
pub(crate) struct SignatureDisplayDetails {
    /// The full signature string
    pub label: String,
    /// Ranges for each parameter within the label
    pub parameter_ranges: Vec<TextRange>,
    /// Names of the parameters in order
    pub parameter_names: Vec<String>,
}

impl<'db> Parameters<'db> {
    fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayParameters<'a, 'db> {
        DisplayParameters {
            parameters: self,
            db,
            settings,
        }
    }
}

struct DisplayParameters<'a, 'db> {
    parameters: &'a Parameters<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayParameters<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        // For `ParamSpec` kind, the parameters still contain `*args` and `**kwargs`, but we
        // display them as `**P` instead, so avoid multiline in that case.
        // TODO: This might change once we support `Concatenate`
        let multiline = self.settings.multiline
            && self.parameters.len() > 1
            && !matches!(
                self.parameters.kind(),
                ParametersKind::Gradual | ParametersKind::ParamSpec(_)
            );
        // Opening parenthesis
        f.write_char('(')?;
        if multiline {
            f.write_str("\n    ")?;
        }
        match self.parameters.kind() {
            ParametersKind::Standard => {
                let mut star_added = false;
                let mut needs_slash = false;
                let mut first = true;
                let arg_separator = if multiline { ",\n    " } else { ", " };

                for parameter in self.parameters.as_slice() {
                    // Handle special separators
                    if !star_added && parameter.is_keyword_only() {
                        if !first {
                            f.write_str(arg_separator)?;
                        }
                        f.write_char('*')?;
                        star_added = true;
                        first = false;
                    }
                    if parameter.is_positional_only() {
                        needs_slash = true;
                    } else if needs_slash {
                        if !first {
                            f.write_str(arg_separator)?;
                        }
                        f.write_char('/')?;
                        needs_slash = false;
                        first = false;
                    }

                    // Add comma before parameter if not first
                    if !first {
                        f.write_str(arg_separator)?;
                    }

                    // Write parameter with range tracking
                    let param_name = parameter
                        .display_name()
                        .map(|name| name.to_string())
                        .unwrap_or_default();
                    parameter
                        .display_with(self.db, self.settings.singleline())
                        .fmt_detailed(&mut f.with_detail(TypeDetail::Parameter(param_name)))?;

                    first = false;
                }

                if needs_slash {
                    if !first {
                        f.write_str(arg_separator)?;
                    }
                    f.write_char('/')?;
                }
            }
            ParametersKind::Gradual | ParametersKind::Top => {
                // We represent gradual form as `...` in the signature, internally the parameters still
                // contain `(*args, **kwargs)` parameters. (Top parameters are displayed the same
                // as gradual parameters, we just wrap the entire signature in `Top[]`.)
                f.write_str("...")?;
            }
            ParametersKind::ParamSpec(typevar) => {
                write!(f, "**{}", typevar.name(self.db))?;
                if let Some(name) = typevar.binding_context(self.db).name(self.db) {
                    write!(f, "@{name}")?;
                }
            }
        }
        if multiline {
            f.write_char('\n')?;
        }
        // Closing parenthesis
        f.write_char(')')
    }
}

impl Display for DisplayParameters<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> Parameter<'db> {
    fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayParameter<'a, 'db> {
        DisplayParameter {
            param: self,
            db,
            settings,
        }
    }
}

struct DisplayParameter<'a, 'db> {
    param: &'a Parameter<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayParameter<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        if let Some(name) = self.param.display_name() {
            f.write_str(&name)?;
            if let Some(annotated_type) = self.param.annotated_type() {
                if self.param.should_annotation_be_displayed() {
                    f.write_str(": ")?;
                    annotated_type
                        .display_with(self.db, self.settings.clone())
                        .fmt_detailed(f)?;
                }
            }
            // Default value can only be specified if `name` is given.
            if let Some(default_type) = self.param.default_type() {
                if self.param.annotated_type().is_some() {
                    f.write_str(" = ")?;
                } else {
                    f.write_str("=")?;
                }
                match default_type {
                    Type::IntLiteral(_)
                    | Type::BooleanLiteral(_)
                    | Type::StringLiteral(_)
                    | Type::EnumLiteral(_)
                    | Type::BytesLiteral(_) => {
                        // For Literal types display the value without `Literal[..]` wrapping
                        let representation =
                            default_type.representation(self.db, self.settings.clone());
                        representation.fmt_detailed(f)?;
                    }
                    Type::NominalInstance(instance) => {
                        // Some key default types like `None` are worth showing
                        let class = instance.class(self.db);

                        match (class, class.known(self.db)) {
                            (_, Some(KnownClass::NoneType)) => {
                                f.with_type(default_type).write_str("None")?;
                            }
                            (_, Some(KnownClass::NoDefaultType)) => {
                                f.with_type(default_type).write_str("NoDefault")?;
                            }
                            _ => f.write_str("...")?,
                        }
                    }
                    _ => f.write_str("...")?,
                }
            }
        } else if let Some(ty) = self.param.annotated_type() {
            // This case is specifically for the `Callable` signature where name and default value
            // cannot be provided.
            ty.display_with(self.db, self.settings.clone())
                .fmt_detailed(f)?;
        }
        Ok(())
    }
}

impl Display for DisplayParameter<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

#[derive(Debug, Copy, Clone)]
struct TruncationPolicy {
    max: usize,
    max_when_elided: usize,
}

impl TruncationPolicy {
    fn display_limit(self, total: usize, preserve_full: bool) -> usize {
        if preserve_full {
            return total;
        }
        let limit = if total > self.max {
            self.max_when_elided
        } else {
            self.max
        };
        limit.min(total)
    }
}

#[derive(Debug)]
struct DisplayOmitted {
    count: usize,
    singular: &'static str,
    plural: &'static str,
}

impl<'db> FmtDetailed<'db> for DisplayOmitted {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let noun = if self.count == 1 {
            self.singular
        } else {
            self.plural
        };
        f.set_invalid_type_annotation();
        write!(f, "... omitted {} {}", self.count, noun)
    }
}

impl Display for DisplayOmitted {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> UnionType<'db> {
    fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayUnionType<'a, 'db> {
        DisplayUnionType {
            db,
            ty: self,
            settings,
        }
    }
}

struct DisplayUnionType<'a, 'db> {
    ty: &'a UnionType<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

const UNION_POLICY: TruncationPolicy = TruncationPolicy {
    max: 5,
    max_when_elided: 3,
};

impl<'db> FmtDetailed<'db> for DisplayUnionType<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        fn is_condensable(ty: Type<'_>) -> bool {
            matches!(
                ty,
                Type::IntLiteral(_)
                    | Type::StringLiteral(_)
                    | Type::BytesLiteral(_)
                    | Type::BooleanLiteral(_)
                    | Type::EnumLiteral(_)
            )
        }

        let elements = self.ty.elements(self.db);

        let condensed_types = elements
            .iter()
            .copied()
            .filter(|element| is_condensable(*element))
            .collect::<Vec<_>>();

        let total_entries =
            usize::from(!condensed_types.is_empty()) + elements.len() - condensed_types.len();

        assert_ne!(total_entries, 0);

        // Done manually because we have a mix of FmtDetailed and Display
        let mut join = f.join(" | ");

        let display_limit =
            UNION_POLICY.display_limit(total_entries, self.settings.preserve_full_unions);

        let mut condensed_types = Some(condensed_types);
        let mut displayed_entries = 0usize;

        for element in elements {
            if displayed_entries >= display_limit {
                break;
            }

            if is_condensable(*element) {
                if let Some(condensed_types) = condensed_types.take() {
                    displayed_entries += 1;
                    join.entry(&DisplayLiteralGroup {
                        literals: condensed_types,
                        db: self.db,
                        settings: self.settings.singleline(),
                    });
                }
            } else {
                displayed_entries += 1;
                join.entry(&DisplayMaybeParenthesizedType {
                    ty: *element,
                    db: self.db,
                    settings: self.settings.singleline(),
                });
            }
        }

        if !self.settings.preserve_full_unions {
            let omitted_entries = total_entries.saturating_sub(displayed_entries);
            if omitted_entries > 0 {
                join.entry(&DisplayOmitted {
                    count: omitted_entries,
                    singular: "union element",
                    plural: "union elements",
                });
            }
        }
        join.finish()
    }
}

impl Display for DisplayUnionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl fmt::Debug for DisplayUnionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}
struct DisplayLiteralGroup<'db> {
    literals: Vec<Type<'db>>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

const LITERAL_POLICY: TruncationPolicy = TruncationPolicy {
    max: 7,
    max_when_elided: 5,
};

impl<'db> FmtDetailed<'db> for DisplayLiteralGroup<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        f.with_type(Type::SpecialForm(SpecialFormType::Literal))
            .write_str("Literal")?;
        f.write_char('[')?;

        let total_entries = self.literals.len();

        let display_limit =
            LITERAL_POLICY.display_limit(total_entries, self.settings.preserve_full_unions);

        let mut join = f.join(", ");

        for lit in self.literals.iter().take(display_limit) {
            let rep = lit.representation(self.db, self.settings.singleline());
            join.entry(&rep);
        }

        if !self.settings.preserve_full_unions {
            let omitted_entries = total_entries.saturating_sub(display_limit);
            if omitted_entries > 0 {
                join.entry(&DisplayOmitted {
                    count: omitted_entries,
                    singular: "literal",
                    plural: "literals",
                });
            }
        }

        join.finish()?;
        f.write_str("]")
    }
}

impl Display for DisplayLiteralGroup<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> IntersectionType<'db> {
    fn display_with<'a>(
        &'a self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayIntersectionType<'a, 'db> {
        DisplayIntersectionType {
            db,
            ty: self,
            settings,
        }
    }
}

struct DisplayIntersectionType<'a, 'db> {
    ty: &'a IntersectionType<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayIntersectionType<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let tys = self
            .ty
            .positive(self.db)
            .iter()
            .map(|&ty| DisplayMaybeNegatedType {
                ty,
                db: self.db,
                settings: self.settings.singleline(),
                negated: false,
            })
            .chain(
                self.ty
                    .negative(self.db)
                    .iter()
                    .map(|&ty| DisplayMaybeNegatedType {
                        ty,
                        db: self.db,
                        settings: self.settings.singleline(),
                        negated: true,
                    }),
            );

        f.set_invalid_type_annotation();
        f.join(" & ").entries(tys).finish()
    }
}

impl Display for DisplayIntersectionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl fmt::Debug for DisplayIntersectionType<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

struct DisplayMaybeNegatedType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    negated: bool,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayMaybeNegatedType<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        if self.negated {
            f.write_str("~")?;
        }
        DisplayMaybeParenthesizedType {
            ty: self.ty,
            db: self.db,
            settings: self.settings.clone(),
        }
        .fmt_detailed(f)
    }
}

impl Display for DisplayMaybeNegatedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

struct DisplayMaybeParenthesizedType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayMaybeParenthesizedType<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let write_parentheses = |f: &mut TypeWriter<'_, '_, 'db>| {
            f.set_invalid_type_annotation();
            f.write_char('(')?;
            self.ty
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f)?;
            f.write_char(')')
        };
        match self.ty {
            Type::Callable(callable)
                if callable.signatures(self.db).overloads.len() == 1
                    && !callable.signatures(self.db).overloads[0]
                        .parameters()
                        .is_top() =>
            {
                write_parentheses(f)
            }
            Type::KnownBoundMethod(_)
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::Union(_) => write_parentheses(f),
            Type::Intersection(intersection) if !intersection.has_one_element(self.db) => {
                write_parentheses(f)
            }
            _ => self
                .ty
                .display_with(self.db, self.settings.clone())
                .fmt_detailed(f),
        }
    }
}

impl Display for DisplayMaybeParenthesizedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

pub(crate) trait TypeArrayDisplay<'db> {
    fn display_with(
        &self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayTypeArray<'_, 'db>;
}

impl<'db> TypeArrayDisplay<'db> for Box<[Type<'db>]> {
    fn display_with(
        &self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayTypeArray<'_, 'db> {
        DisplayTypeArray {
            types: self,
            db,
            settings,
        }
    }
}

impl<'db> TypeArrayDisplay<'db> for Vec<Type<'db>> {
    fn display_with(
        &self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayTypeArray<'_, 'db> {
        DisplayTypeArray {
            types: self,
            db,
            settings,
        }
    }
}

impl<'db> TypeArrayDisplay<'db> for [Type<'db>] {
    fn display_with(
        &self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayTypeArray<'_, 'db> {
        DisplayTypeArray {
            types: self,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayTypeArray<'b, 'db> {
    types: &'b [Type<'db>],
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl<'db> FmtDetailed<'db> for DisplayTypeArray<'_, 'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        f.join(", ")
            .entries(
                self.types
                    .iter()
                    .map(|ty| ty.display_with(self.db, self.settings.singleline())),
            )
            .finish()
    }
}

impl Display for DisplayTypeArray<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> StringLiteralType<'db> {
    fn display_with(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayStringLiteralType<'db> {
        DisplayStringLiteralType {
            string: self.value(db),
            settings,
        }
    }
}

struct DisplayStringLiteralType<'db> {
    string: &'db str,
    #[expect(dead_code)]
    settings: DisplaySettings<'db>,
}

impl Display for DisplayStringLiteralType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_char('"')?;
        for ch in self.string.chars() {
            match ch {
                // `escape_debug` will escape even single quotes, which is not necessary for our
                // use case as we are already using double quotes to wrap the string.
                '\'' => f.write_char('\''),
                _ => ch.escape_debug().fmt(f),
            }?;
        }
        f.write_char('"')
    }
}

pub(crate) struct DisplayKnownInstanceRepr<'db> {
    pub(crate) known_instance: KnownInstanceType<'db>,
    pub(crate) db: &'db dyn Db,
}

impl<'db> KnownInstanceType<'db> {
    pub(crate) fn display_with(
        self,
        db: &'db dyn Db,
        _settings: DisplaySettings<'db>,
    ) -> DisplayKnownInstanceRepr<'db> {
        DisplayKnownInstanceRepr {
            known_instance: self,
            db,
        }
    }
}

impl Display for DisplayKnownInstanceRepr<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_detailed(&mut TypeWriter::Formatter(f))
    }
}

impl<'db> FmtDetailed<'db> for DisplayKnownInstanceRepr<'db> {
    fn fmt_detailed(&self, f: &mut TypeWriter<'_, '_, 'db>) -> fmt::Result {
        let ty = Type::KnownInstance(self.known_instance);
        match self.known_instance {
            KnownInstanceType::SubscriptedProtocol(generic_context) => {
                f.set_invalid_type_annotation();
                f.write_str("<special-form '")?;
                f.with_type(Type::SpecialForm(SpecialFormType::Protocol))
                    .write_str("typing.Protocol")?;
                generic_context.display(self.db).fmt_detailed(f)?;
                f.write_str("'>")
            }
            KnownInstanceType::SubscriptedGeneric(generic_context) => {
                f.set_invalid_type_annotation();
                f.write_str("<special-form '")?;
                f.with_type(Type::SpecialForm(SpecialFormType::Generic))
                    .write_str("typing.Generic")?;
                generic_context.display(self.db).fmt_detailed(f)?;
                f.write_str("'>")
            }
            KnownInstanceType::TypeAliasType(alias) => {
                if let Some(specialization) = alias.specialization(self.db) {
                    f.set_invalid_type_annotation();
                    f.write_str("<type alias '")?;
                    f.with_type(ty).write_str(alias.name(self.db))?;
                    specialization
                        .display_short(self.db, TupleSpecialization::No, DisplaySettings::default())
                        .fmt_detailed(f)?;
                    f.write_str("'>")
                } else {
                    f.with_type(ty).write_str("TypeAliasType")
                }
            }
            // This is a legacy `TypeVar` _outside_ of any generic class or function, so we render
            // it as an instance of `typing.TypeVar`. Inside of a generic class or function, we'll
            // have a `Type::TypeVar(_)`, which is rendered as the typevar's name.
            KnownInstanceType::TypeVar(typevar_instance) => {
                if typevar_instance.kind(self.db).is_paramspec() {
                    f.with_type(ty).write_str("ParamSpec")
                } else {
                    f.with_type(ty).write_str("TypeVar")
                }
            }
            KnownInstanceType::Deprecated(_) => f.write_str("warnings.deprecated"),
            KnownInstanceType::Field(field) => {
                f.with_type(ty).write_str("dataclasses.Field")?;
                if let Some(default_ty) = field.default_type(self.db) {
                    f.write_char('[')?;
                    write!(f.with_type(default_ty), "{}", default_ty.display(self.db))?;
                    f.write_char(']')?;
                }
                Ok(())
            }
            KnownInstanceType::ConstraintSet(_) => {
                f.with_type(ty).write_str("ty_extensions.ConstraintSet")
            }
            KnownInstanceType::GenericContext(generic_context) => {
                f.with_type(ty).write_str("ty_extensions.GenericContext")?;
                write!(f, "{}", generic_context.display_full(self.db))
            }
            KnownInstanceType::Specialization(specialization) => {
                // Normalize for consistent output across CI platforms
                f.with_type(ty).write_str("ty_extensions.Specialization")?;
                write!(f, "{}", specialization.display_full(self.db))
            }
            KnownInstanceType::UnionType(union) => {
                f.set_invalid_type_annotation();
                f.write_char('<')?;
                f.with_type(KnownClass::UnionType.to_class_literal(self.db))
                    .write_str("types.UnionType")?;
                f.write_str(" special-form")?;
                if let Ok(ty) = union.union_type(self.db) {
                    f.write_str(" '")?;
                    ty.display(self.db).fmt_detailed(f)?;
                    f.write_char('\'')?;
                }
                f.write_char('>')
            }
            KnownInstanceType::Literal(inner) => {
                f.set_invalid_type_annotation();
                f.write_str("<special-form '")?;
                inner.inner(self.db).display(self.db).fmt_detailed(f)?;
                f.write_str("'>")
            }
            KnownInstanceType::Annotated(inner) => {
                f.set_invalid_type_annotation();
                f.write_str("<special-form '")?;
                f.with_type(Type::SpecialForm(SpecialFormType::Annotated))
                    .write_str("typing.Annotated")?;
                f.write_char('[')?;
                inner.inner(self.db).display(self.db).fmt_detailed(f)?;
                f.write_str(", <metadata>]'>")
            }
            KnownInstanceType::Callable(callable) => {
                f.set_invalid_type_annotation();
                f.write_char('<')?;
                f.with_type(Type::SpecialForm(SpecialFormType::Callable))
                    .write_str("typing.Callable")?;
                f.write_str(" special-form '")?;
                callable.display(self.db).fmt_detailed(f)?;
                f.write_str("'>")
            }
            KnownInstanceType::TypeGenericAlias(inner) => {
                f.set_invalid_type_annotation();
                f.write_str("<special-form '")?;
                f.with_type(KnownClass::Type.to_class_literal(self.db))
                    .write_str("type")?;
                f.write_char('[')?;
                inner.inner(self.db).display(self.db).fmt_detailed(f)?;
                f.write_str("]'>")
            }
            KnownInstanceType::LiteralStringAlias(_) => f
                .with_type(KnownClass::Str.to_class_literal(self.db))
                .write_str("str"),
            KnownInstanceType::NewType(declaration) => {
                f.set_invalid_type_annotation();
                f.write_char('<')?;
                f.with_type(KnownClass::NewType.to_class_literal(self.db))
                    .write_str("NewType")?;
                f.write_str(" pseudo-class '")?;
                f.with_type(ty).write_str(declaration.name(self.db))?;
                f.write_str("'>")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_python_ast::name::Name;

    use crate::Db;
    use crate::db::tests::setup_db;
    use crate::place::typing_extensions_symbol;
    use crate::types::typed_dict::{
        SynthesizedTypedDictType, TypedDictFieldBuilder, TypedDictSchema,
    };
    use crate::types::{KnownClass, Parameter, Parameters, Signature, Type, TypedDictType};

    #[test]
    fn string_literal_display() {
        let db = setup_db();

        assert_eq!(
            Type::string_literal(&db, r"\n").display(&db).to_string(),
            r#"Literal["\\n"]"#
        );
        assert_eq!(
            Type::string_literal(&db, "'").display(&db).to_string(),
            r#"Literal["'"]"#
        );
        assert_eq!(
            Type::string_literal(&db, r#"""#).display(&db).to_string(),
            r#"Literal["\""]"#
        );
    }

    #[test]
    fn synthesized_protocol_display() {
        let db = setup_db();

        // Call `.normalized()` to turn the class-based protocol into a nameless synthesized one.
        let supports_index_synthesized = KnownClass::SupportsIndex.to_instance(&db).normalized(&db);
        assert_eq!(
            supports_index_synthesized.display(&db).to_string(),
            "<Protocol with members '__index__'>"
        );

        let iterator_synthesized = typing_extensions_symbol(&db, "Iterator")
            .place
            .ignore_possibly_undefined()
            .unwrap()
            .to_instance(&db)
            .unwrap()
            .normalized(&db); // Call `.normalized()` to turn the class-based protocol into a nameless synthesized one.

        assert_eq!(
            iterator_synthesized.display(&db).to_string(),
            "<Protocol with members '__iter__', '__next__'>"
        );
    }

    #[test]
    fn synthesized_typeddict_display() {
        let db = setup_db();

        let mut items = TypedDictSchema::default();
        items.insert(
            Name::new("foo"),
            TypedDictFieldBuilder::new(Type::IntLiteral(42))
                .required(true)
                .build(),
        );
        items.insert(
            Name::new("bar"),
            TypedDictFieldBuilder::new(Type::string_literal(&db, "hello"))
                .required(true)
                .build(),
        );

        let synthesized = SynthesizedTypedDictType::new(&db, items);
        let type_ = Type::TypedDict(TypedDictType::Synthesized(synthesized));
        // Fields are sorted internally, even prior to normalization.
        assert_eq!(
            type_.display(&db).to_string(),
            "<TypedDict with items 'bar', 'foo'>",
        );
        assert_eq!(
            type_.normalized(&db).display(&db).to_string(),
            "<TypedDict with items 'bar', 'foo'>",
        );
    }

    fn display_signature<'db>(
        db: &'db dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
        return_ty: Option<Type<'db>>,
    ) -> String {
        Signature::new(Parameters::new(db, parameters), return_ty)
            .display(db)
            .to_string()
    }

    fn display_signature_multiline<'db>(
        db: &'db dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
        return_ty: Option<Type<'db>>,
    ) -> String {
        Signature::new(Parameters::new(db, parameters), return_ty)
            .display_with(db, super::DisplaySettings::default().multiline())
            .to_string()
    }

    #[test]
    fn signature_display() {
        let db = setup_db();

        // Empty parameters with no return type.
        assert_snapshot!(display_signature(&db, [], None), @"() -> Unknown");

        // Empty parameters with a return type.
        assert_snapshot!(
            display_signature(&db, [], Some(Type::none(&db))),
            @"() -> None"
        );

        // Single parameter type (no name) with a return type.
        assert_snapshot!(
            display_signature(
                &db,
                [Parameter::positional_only(None).with_annotated_type(Type::none(&db))],
                Some(Type::none(&db))
            ),
            @"(None, /) -> None"
        );

        // Two parameters where one has annotation and the other doesn't.
        assert_snapshot!(
            display_signature(
                &db,
                [
                    Parameter::positional_or_keyword(Name::new_static("x"))
                        .with_default_type(KnownClass::Int.to_instance(&db)),
                    Parameter::positional_or_keyword(Name::new_static("y"))
                        .with_annotated_type(KnownClass::Str.to_instance(&db))
                        .with_default_type(KnownClass::Str.to_instance(&db)),
                ],
                Some(Type::none(&db))
            ),
            @"(x=..., y: str = ...) -> None"
        );

        // All positional only parameters.
        assert_snapshot!(
            display_signature(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("x"))),
                    Parameter::positional_only(Some(Name::new_static("y"))),
                ],
                Some(Type::none(&db))
            ),
            @"(x, y, /) -> None"
        );

        // Positional-only parameters mixed with non-positional-only parameters.
        assert_snapshot!(
            display_signature(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("x"))),
                    Parameter::positional_or_keyword(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            @"(x, /, y) -> None"
        );

        // All keyword-only parameters.
        assert_snapshot!(
            display_signature(
                &db,
                [
                    Parameter::keyword_only(Name::new_static("x")),
                    Parameter::keyword_only(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            @"(*, x, y) -> None"
        );

        // Keyword-only parameters mixed with non-keyword-only parameters.
        assert_snapshot!(
            display_signature(
                &db,
                [
                    Parameter::positional_or_keyword(Name::new_static("x")),
                    Parameter::keyword_only(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            @"(x, *, y) -> None"
        );

        // A mix of all parameter kinds.
        assert_snapshot!(
            display_signature(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("a"))),
                    Parameter::positional_only(Some(Name::new_static("b")))
                        .with_annotated_type(KnownClass::Int.to_instance(&db)),
                    Parameter::positional_only(Some(Name::new_static("c")))
                        .with_default_type(Type::IntLiteral(1)),
                    Parameter::positional_only(Some(Name::new_static("d")))
                        .with_annotated_type(KnownClass::Int.to_instance(&db))
                        .with_default_type(Type::IntLiteral(2)),
                    Parameter::positional_or_keyword(Name::new_static("e"))
                        .with_default_type(Type::IntLiteral(3)),
                    Parameter::positional_or_keyword(Name::new_static("f"))
                        .with_annotated_type(KnownClass::Int.to_instance(&db))
                        .with_default_type(Type::IntLiteral(4)),
                    Parameter::variadic(Name::new_static("args"))
                        .with_annotated_type(Type::object()),
                    Parameter::keyword_only(Name::new_static("g"))
                        .with_default_type(Type::IntLiteral(5)),
                    Parameter::keyword_only(Name::new_static("h"))
                        .with_annotated_type(KnownClass::Int.to_instance(&db))
                        .with_default_type(Type::IntLiteral(6)),
                    Parameter::keyword_variadic(Name::new_static("kwargs"))
                        .with_annotated_type(KnownClass::Str.to_instance(&db)),
                ],
                Some(KnownClass::Bytes.to_instance(&db))
            ),
            @"(a, b: int, c=1, d: int = 2, /, e=3, f: int = 4, *args: object, *, g=5, h: int = 6, **kwargs: str) -> bytes"
        );
    }

    #[test]
    fn signature_display_multiline() {
        let db = setup_db();

        // Empty parameters with no return type.
        assert_snapshot!(display_signature_multiline(&db, [], None), @"() -> Unknown");

        // Empty parameters with a return type.
        assert_snapshot!(
            display_signature_multiline(&db, [], Some(Type::none(&db))),
            @"() -> None"
        );

        // Single parameter type (no name) with a return type.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [Parameter::positional_only(None).with_annotated_type(Type::none(&db))],
                Some(Type::none(&db))
            ),
            @"(None, /) -> None"
        );

        // Two parameters where one has annotation and the other doesn't.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [
                    Parameter::positional_or_keyword(Name::new_static("x"))
                        .with_default_type(KnownClass::Int.to_instance(&db)),
                    Parameter::positional_or_keyword(Name::new_static("y"))
                        .with_annotated_type(KnownClass::Str.to_instance(&db))
                        .with_default_type(KnownClass::Str.to_instance(&db)),
                ],
                Some(Type::none(&db))
            ),
            @r"
        (
            x=...,
            y: str = ...
        ) -> None
        "
        );

        // All positional only parameters.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("x"))),
                    Parameter::positional_only(Some(Name::new_static("y"))),
                ],
                Some(Type::none(&db))
            ),
            @r"
        (
            x,
            y,
            /
        ) -> None
        "
        );

        // Positional-only parameters mixed with non-positional-only parameters.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("x"))),
                    Parameter::positional_or_keyword(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            @r"
        (
            x,
            /,
            y
        ) -> None
        "
        );

        // All keyword-only parameters.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [
                    Parameter::keyword_only(Name::new_static("x")),
                    Parameter::keyword_only(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            @r"
        (
            *,
            x,
            y
        ) -> None
        "
        );

        // Keyword-only parameters mixed with non-keyword-only parameters.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [
                    Parameter::positional_or_keyword(Name::new_static("x")),
                    Parameter::keyword_only(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            @r"
        (
            x,
            *,
            y
        ) -> None
        "
        );

        // A mix of all parameter kinds.
        assert_snapshot!(
            display_signature_multiline(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("a"))),
                    Parameter::positional_only(Some(Name::new_static("b")))
                        .with_annotated_type(KnownClass::Int.to_instance(&db)),
                    Parameter::positional_only(Some(Name::new_static("c")))
                        .with_default_type(Type::IntLiteral(1)),
                    Parameter::positional_only(Some(Name::new_static("d")))
                        .with_annotated_type(KnownClass::Int.to_instance(&db))
                        .with_default_type(Type::IntLiteral(2)),
                    Parameter::positional_or_keyword(Name::new_static("e"))
                        .with_default_type(Type::IntLiteral(3)),
                    Parameter::positional_or_keyword(Name::new_static("f"))
                        .with_annotated_type(KnownClass::Int.to_instance(&db))
                        .with_default_type(Type::IntLiteral(4)),
                    Parameter::variadic(Name::new_static("args"))
                        .with_annotated_type(Type::object()),
                    Parameter::keyword_only(Name::new_static("g"))
                        .with_default_type(Type::IntLiteral(5)),
                    Parameter::keyword_only(Name::new_static("h"))
                        .with_annotated_type(KnownClass::Int.to_instance(&db))
                        .with_default_type(Type::IntLiteral(6)),
                    Parameter::keyword_variadic(Name::new_static("kwargs"))
                        .with_annotated_type(KnownClass::Str.to_instance(&db)),
                ],
                Some(KnownClass::Bytes.to_instance(&db))
            ),
            @r"
        (
            a,
            b: int,
            c=1,
            d: int = 2,
            /,
            e=3,
            f: int = 4,
            *args: object,
            *,
            g=5,
            h: int = 6,
            **kwargs: str
        ) -> bytes
        "
        );
    }
}
