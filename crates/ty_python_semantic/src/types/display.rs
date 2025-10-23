//! Display implementations for types.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::fmt::{self, Display, Formatter, Write};
use std::rc::Rc;

use ruff_db::display::FormatterJoinExtension;
use ruff_db::files::FilePath;
use ruff_db::source::line_index;
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_literal::escape::AsciiEscape;
use ruff_text_size::{TextRange, TextSize};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::Db;
use crate::module_resolver::file_to_module;
use crate::semantic_index::{scope::ScopeKind, semantic_index};
use crate::types::class::{ClassLiteral, ClassType, GenericAlias};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::tuple::TupleSpec;
use crate::types::visitor::TypeVisitor;
use crate::types::{
    BoundTypeVarIdentity, CallableType, IntersectionType, KnownBoundMethodType, KnownClass,
    MaterializationKind, Protocol, ProtocolInstanceType, StringLiteralType, SubclassOfInner, Type,
    UnionType, WrapperDescriptorKind, visitor,
};
use ruff_db::parsed::parsed_module;

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
    pub fn from_possibly_ambiguous_type_pair(
        db: &'db dyn Db,
        type_1: Type<'db>,
        type_2: Type<'db>,
    ) -> Self {
        let collector = AmbiguousClassCollector::default();
        collector.visit_type(db, type_1);
        collector.visit_type(db, type_2);

        Self {
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
            ..Self::default()
        }
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
                            let qualified_name_components = class.qualified_name_components(db);
                            if existing.qualified_name_components(db) == qualified_name_components {
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
                            let new_components = class.qualified_name_components(db);
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

impl<'db> super::visitor::TypeVisitor<'db> for AmbiguousClassCollector<'db> {
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
            settings: DisplaySettings::default(),
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

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let representation = self.ty.representation(self.db, self.settings.clone());
        match self.ty {
            Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::EnumLiteral(_) => {
                write!(f, "Literal[{representation}]")
            }
            _ => representation.fmt(f),
        }
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

    /// Returns the components of the qualified name of this class, excluding this class itself.
    ///
    /// For example, calling this method on a class `C` in the module `a.b` would return
    /// `["a", "b"]`. Calling this method on a class `D` inside the namespace of a method
    /// `m` inside the namespace of a class `C` in the module `a.b` would return
    /// `["a", "b", "C", "<locals of function 'm'>"]`.
    fn qualified_name_components(self, db: &'db dyn Db) -> Vec<String> {
        let body_scope = self.body_scope(db);
        let file = body_scope.file(db);
        let module_ast = parsed_module(db, file).load(db);
        let index = semantic_index(db, file);
        let file_scope_id = body_scope.file_scope_id(db);

        let mut name_parts = vec![];

        // Skips itself
        for (_, ancestor_scope) in index.ancestor_scopes(file_scope_id).skip(1) {
            let node = ancestor_scope.node();

            match ancestor_scope.kind() {
                ScopeKind::Class => {
                    if let Some(class_def) = node.as_class() {
                        name_parts.push(class_def.node(&module_ast).name.as_str().to_string());
                    }
                }
                ScopeKind::Function => {
                    if let Some(function_def) = node.as_function() {
                        name_parts.push(format!(
                            "<locals of function '{}'>",
                            function_def.node(&module_ast).name.as_str()
                        ));
                    }
                }
                _ => {}
            }
        }

        if let Some(module) = file_to_module(db, file) {
            let module_name = module.name(db);
            name_parts.push(module_name.as_str().to_string());
        }

        name_parts.reverse();
        name_parts
    }
}

struct ClassDisplay<'db> {
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    settings: DisplaySettings<'db>,
}

impl Display for ClassDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let qualification_level = self.settings.qualified.get(&**self.class.name(self.db));
        if qualification_level.is_some() {
            for parent in self.class.qualified_name_components(self.db) {
                f.write_str(&parent)?;
                f.write_char('.')?;
            }
        }
        f.write_str(self.class.name(self.db))?;
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
            write!(f, " @ {path}:{line_number}")?;
        }
        Ok(())
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
        match self.ty {
            Type::Dynamic(dynamic) => dynamic.fmt(f),
            Type::Never => f.write_str("Never"),
            Type::NominalInstance(instance) => {
                let class = instance.class(self.db);

                match (class, class.known(self.db)) {
                    (_, Some(KnownClass::NoneType)) => f.write_str("None"),
                    (_, Some(KnownClass::NoDefaultType)) => f.write_str("NoDefault"),
                    (ClassType::Generic(alias), Some(KnownClass::Tuple)) => alias
                        .specialization(self.db)
                        .tuple(self.db)
                        .expect("Specialization::tuple() should always return `Some()` for `KnownClass::Tuple`")
                        .display_with(self.db, self.settings.clone())
                        .fmt(f),
                    (ClassType::NonGeneric(class), _) => {
                        class.display_with(self.db, self.settings.clone()).fmt(f)
                    },
                    (ClassType::Generic(alias), _) => alias.display_with(self.db, self.settings.clone()).fmt(f),
                }
            }
            Type::ProtocolInstance(protocol) => match protocol.inner {
                Protocol::FromClass(ClassType::NonGeneric(class)) => {
                    class.display_with(self.db, self.settings.clone()).fmt(f)
                }
                Protocol::FromClass(ClassType::Generic(alias)) => {
                    alias.display_with(self.db, self.settings.clone()).fmt(f)
                }
                Protocol::Synthesized(synthetic) => {
                    f.write_str("<Protocol with members ")?;
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
            Type::PropertyInstance(_) => f.write_str("property"),
            Type::ModuleLiteral(module) => {
                write!(f, "<module '{}'>", module.module(self.db).name(self.db))
            }
            Type::ClassLiteral(class) => write!(
                f,
                "<class '{}'>",
                class.display_with(self.db, self.settings.clone())
            ),
            Type::GenericAlias(generic) => write!(
                f,
                "<class '{}'>",
                generic.display_with(self.db, self.settings.singleline())
            ),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(ClassType::NonGeneric(class)) => {
                    write!(
                        f,
                        "type[{}]",
                        class.display_with(self.db, self.settings.clone())
                    )
                }
                SubclassOfInner::Class(ClassType::Generic(alias)) => {
                    write!(
                        f,
                        "type[{}]",
                        alias.display_with(self.db, self.settings.singleline())
                    )
                }
                SubclassOfInner::Dynamic(dynamic) => write!(f, "type[{dynamic}]"),
            },
            Type::SpecialForm(special_form) => special_form.fmt(f),
            Type::KnownInstance(known_instance) => known_instance.repr(self.db).fmt(f),
            Type::FunctionLiteral(function) => {
                function.display_with(self.db, self.settings.clone()).fmt(f)
            }
            Type::Callable(callable) => {
                callable.display_with(self.db, self.settings.clone()).fmt(f)
            }
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

                        write!(
                            f,
                            "bound method {instance}.{method}{type_parameters}{signature}",
                            method = function.name(self.db),
                            instance = self_ty.display_with(self.db, self.settings.singleline()),
                            type_parameters = type_parameters,
                            signature = signature
                                .bind_self(self.db, Some(typing_self_ty))
                                .display_with(self.db, self.settings.clone())
                        )
                    }
                    signatures => {
                        // TODO: How to display overloads?
                        if !self.settings.multiline {
                            f.write_str("Overload[")?;
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
                        if !self.settings.multiline {
                            f.write_str("]")?;
                        }
                        Ok(())
                    }
                }
            }
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderGet(function)) => {
                write!(
                    f,
                    "<method-wrapper `__get__` of `{function}`>",
                    function = function.name(self.db),
                )
            }
            Type::KnownBoundMethod(KnownBoundMethodType::FunctionTypeDunderCall(function)) => {
                write!(
                    f,
                    "<method-wrapper `__call__` of `{function}`>",
                    function = function.name(self.db),
                )
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderGet(_)) => {
                f.write_str("<method-wrapper `__get__` of `property` object>")
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PropertyDunderSet(_)) => {
                f.write_str("<method-wrapper `__set__` of `property` object>")
            }
            Type::KnownBoundMethod(KnownBoundMethodType::StrStartswith(_)) => {
                f.write_str("<method-wrapper `startswith` of `str` object>")
            }
            Type::KnownBoundMethod(KnownBoundMethodType::PathOpen) => {
                f.write_str("bound method `Path.open`")
            }
            Type::WrapperDescriptor(kind) => {
                let (method, object) = match kind {
                    WrapperDescriptorKind::FunctionTypeDunderGet => ("__get__", "function"),
                    WrapperDescriptorKind::PropertyDunderGet => ("__get__", "property"),
                    WrapperDescriptorKind::PropertyDunderSet => ("__set__", "property"),
                };
                write!(f, "<wrapper-descriptor `{method}` of `{object}` objects>")
            }
            Type::DataclassDecorator(_) => {
                f.write_str("<decorator produced by dataclass-like function>")
            }
            Type::DataclassTransformer(_) => {
                f.write_str("<decorator produced by typing.dataclass_transform>")
            }
            Type::Union(union) => union.display_with(self.db, self.settings.clone()).fmt(f),
            Type::Intersection(intersection) => intersection
                .display_with(self.db, self.settings.clone())
                .fmt(f),
            Type::IntLiteral(n) => n.fmt(f),
            Type::BooleanLiteral(boolean) => f.write_str(if boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => {
                string.display_with(self.db, self.settings.clone()).fmt(f)
            }
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape = AsciiEscape::with_preferred_quote(bytes.value(self.db), Quote::Double);

                escape.bytes_repr(TripleQuotes::No).write(f)
            }
            Type::EnumLiteral(enum_literal) => write!(
                f,
                "{enum_class}.{literal_name}",
                enum_class = enum_literal
                    .enum_class(self.db)
                    .display_with(self.db, self.settings.clone()),
                literal_name = enum_literal.name(self.db)
            ),
            Type::TypeVar(bound_typevar) => bound_typevar.identity(self.db).display(self.db).fmt(f),
            Type::AlwaysTruthy => f.write_str("AlwaysTruthy"),
            Type::AlwaysFalsy => f.write_str("AlwaysFalsy"),
            Type::BoundSuper(bound_super) => {
                write!(
                    f,
                    "<super: {pivot}, {owner}>",
                    pivot = Type::from(bound_super.pivot_class(self.db))
                        .display_with(self.db, self.settings.singleline()),
                    owner = Type::from(bound_super.owner(self.db))
                        .display_with(self.db, self.settings.singleline())
                )
            }
            Type::TypeIs(type_is) => {
                f.write_str("TypeIs[")?;
                type_is
                    .return_type(self.db)
                    .display_with(self.db, self.settings.singleline())
                    .fmt(f)?;
                if let Some(name) = type_is.place_name(self.db) {
                    f.write_str(" @ ")?;
                    f.write_str(&name)?;
                }
                f.write_str("]")
            }
            Type::TypedDict(typed_dict) => typed_dict
                .defining_class()
                .class_literal(self.db)
                .0
                .display_with(self.db, self.settings.clone())
                .fmt(f),
            Type::TypeAlias(alias) => {
                f.write_str(alias.name(self.db))?;
                match alias.specialization(self.db) {
                    None => Ok(()),
                    Some(specialization) => specialization
                        .display_short(self.db, TupleSpecialization::No, self.settings.clone())
                        .fmt(f),
                }
            }
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
        Ok(())
    }
}

impl<'db> TupleSpec<'db> {
    pub(crate) fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayTuple<'db> {
        DisplayTuple {
            tuple: self,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayTuple<'db> {
    tuple: &'db TupleSpec<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayTuple<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("tuple[")?;
        match self.tuple {
            TupleSpec::Fixed(tuple) => {
                let elements = tuple.elements_slice();
                if elements.is_empty() {
                    f.write_str("()")?;
                } else {
                    elements
                        .display_with(self.db, self.settings.singleline())
                        .fmt(f)?;
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
                if !tuple.prefix.is_empty() {
                    tuple
                        .prefix
                        .display_with(self.db, self.settings.singleline())
                        .fmt(f)?;
                    f.write_str(", ")?;
                }
                if !tuple.prefix.is_empty() || !tuple.suffix.is_empty() {
                    f.write_str("*tuple[")?;
                }
                tuple
                    .variable
                    .display_with(self.db, self.settings.singleline())
                    .fmt(f)?;
                f.write_str(", ...")?;
                if !tuple.prefix.is_empty() || !tuple.suffix.is_empty() {
                    f.write_str("]")?;
                }
                if !tuple.suffix.is_empty() {
                    f.write_str(", ")?;
                    tuple
                        .suffix
                        .display_with(self.db, self.settings.singleline())
                        .fmt(f)?;
                }
            }
        }
        f.write_str("]")
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

impl Display for DisplayOverloadLiteral<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let signature = self.literal.signature(self.db);
        let type_parameters = DisplayOptionalGenericContext {
            generic_context: signature.generic_context.as_ref(),
            db: self.db,
            settings: self.settings.clone(),
        };

        write!(
            f,
            "def {name}{type_parameters}{signature}",
            name = self.literal.name(self.db),
            type_parameters = type_parameters,
            signature = signature.display_with(self.db, self.settings.clone())
        )
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

impl Display for DisplayFunctionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let signature = self.ty.signature(self.db);

        match signature.overloads.as_slice() {
            [signature] => {
                let type_parameters = DisplayOptionalGenericContext {
                    generic_context: signature.generic_context.as_ref(),
                    db: self.db,
                    settings: self.settings.clone(),
                };

                write!(
                    f,
                    "def {name}{type_parameters}{signature}",
                    name = self.ty.name(self.db),
                    type_parameters = type_parameters,
                    signature = signature.display_with(self.db, self.settings.clone())
                )
            }
            signatures => {
                // TODO: How to display overloads?
                if !self.settings.multiline {
                    f.write_str("Overload[")?;
                }
                let separator = if self.settings.multiline { "\n" } else { ", " };
                let mut join = f.join(separator);
                for signature in signatures {
                    join.entry(&signature.display_with(self.db, self.settings.clone()));
                }
                if !self.settings.multiline {
                    f.write_str("]")?;
                }
                Ok(())
            }
        }
    }
}

impl<'db> GenericAlias<'db> {
    pub(crate) fn display(&'db self, db: &'db dyn Db) -> DisplayGenericAlias<'db> {
        self.display_with(db, DisplaySettings::default())
    }

    pub(crate) fn display_with(
        &'db self,
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

impl Display for DisplayGenericAlias<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(tuple) = self.specialization.tuple(self.db) {
            tuple.display_with(self.db, self.settings.clone()).fmt(f)
        } else {
            let prefix = match self.specialization.materialization_kind(self.db) {
                None => "",
                Some(MaterializationKind::Top) => "Top[",
                Some(MaterializationKind::Bottom) => "Bottom[",
            };
            let suffix = match self.specialization.materialization_kind(self.db) {
                None => "",
                Some(_) => "]",
            };
            write!(
                f,
                "{prefix}{origin}{specialization}{suffix}",
                prefix = prefix,
                origin = self.origin.display_with(self.db, self.settings.clone()),
                specialization = self.specialization.display_short(
                    self.db,
                    TupleSpecialization::from_class(self.db, self.origin),
                    self.settings.clone()
                ),
                suffix = suffix,
            )
        }
    }
}

impl<'db> GenericContext<'db> {
    pub fn display(&'db self, db: &'db dyn Db) -> DisplayGenericContext<'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }
    pub fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayGenericContext<'db> {
        DisplayGenericContext {
            generic_context: self,
            db,
            settings,
        }
    }
}

struct DisplayOptionalGenericContext<'db> {
    generic_context: Option<&'db GenericContext<'db>>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayOptionalGenericContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(generic_context) = self.generic_context {
            DisplayGenericContext {
                generic_context,
                db: self.db,
                settings: self.settings.clone(),
            }
            .fmt(f)
        } else {
            Ok(())
        }
    }
}

pub struct DisplayGenericContext<'db> {
    generic_context: &'db GenericContext<'db>,
    db: &'db dyn Db,
    #[expect(dead_code)]
    settings: DisplaySettings<'db>,
}

impl Display for DisplayGenericContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
            f.write_str(bound_typevar.typevar(self.db).name(self.db))?;
        }
        f.write_char(']')
    }
}

impl<'db> Specialization<'db> {
    pub fn display(&'db self, db: &'db dyn Db) -> DisplaySpecialization<'db> {
        self.display_short(db, TupleSpecialization::No, DisplaySettings::default())
    }

    /// Renders the specialization as it would appear in a subscript expression, e.g. `[int, str]`.
    pub fn display_short(
        &'db self,
        db: &'db dyn Db,
        tuple_specialization: TupleSpecialization,
        settings: DisplaySettings<'db>,
    ) -> DisplaySpecialization<'db> {
        DisplaySpecialization {
            types: self.types(db),
            db,
            tuple_specialization,
            settings,
        }
    }
}

pub struct DisplaySpecialization<'db> {
    types: &'db [Type<'db>],
    db: &'db dyn Db,
    tuple_specialization: TupleSpecialization,
    settings: DisplaySettings<'db>,
}

impl Display for DisplaySpecialization<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (idx, ty) in self.types.iter().enumerate() {
            if idx > 0 {
                f.write_str(", ")?;
            }
            ty.display_with(self.db, self.settings.clone()).fmt(f)?;
        }
        if self.tuple_specialization.is_yes() {
            f.write_str(", ...")?;
        }
        f.write_char(']')
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
    pub(crate) fn display(&'db self, db: &'db dyn Db) -> DisplayCallableType<'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }

    pub(crate) fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayCallableType<'db> {
        DisplayCallableType {
            signatures: self.signatures(db),
            db,
            settings,
        }
    }
}

pub(crate) struct DisplayCallableType<'db> {
    signatures: &'db CallableSignature<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayCallableType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.signatures.overloads.as_slice() {
            [signature] => signature
                .display_with(self.db, self.settings.clone())
                .fmt(f),
            signatures => {
                // TODO: How to display overloads?
                if !self.settings.multiline {
                    f.write_str("Overload[")?;
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
                Ok(())
            }
        }
    }
}

impl<'db> Signature<'db> {
    pub(crate) fn display(&'db self, db: &'db dyn Db) -> DisplaySignature<'db> {
        Self::display_with(self, db, DisplaySettings::default())
    }

    pub(crate) fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplaySignature<'db> {
        DisplaySignature {
            parameters: self.parameters(),
            return_ty: self.return_ty,
            db,
            settings,
        }
    }
}

pub(crate) struct DisplaySignature<'db> {
    parameters: &'db Parameters<'db>,
    return_ty: Option<Type<'db>>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl DisplaySignature<'_> {
    /// Get detailed display information including component ranges
    pub(crate) fn to_string_parts(&self) -> SignatureDisplayDetails {
        let mut writer = SignatureWriter::Details(SignatureDetailsWriter::new());
        self.write_signature(&mut writer).unwrap();

        match writer {
            SignatureWriter::Details(details) => details.finish(),
            SignatureWriter::Formatter(_) => unreachable!("Expected Details variant"),
        }
    }

    /// Internal method to write signature with the signature writer
    fn write_signature(&self, writer: &mut SignatureWriter) -> fmt::Result {
        let multiline = self.settings.multiline && self.parameters.len() > 1;
        // Opening parenthesis
        writer.write_char('(')?;
        if multiline {
            writer.write_str("\n    ")?;
        }
        if self.parameters.is_gradual() {
            // We represent gradual form as `...` in the signature, internally the parameters still
            // contain `(*args, **kwargs)` parameters.
            writer.write_str("...")?;
        } else {
            let mut star_added = false;
            let mut needs_slash = false;
            let mut first = true;
            let arg_separator = if multiline { ",\n    " } else { ", " };

            for parameter in self.parameters.as_slice() {
                // Handle special separators
                if !star_added && parameter.is_keyword_only() {
                    if !first {
                        writer.write_str(arg_separator)?;
                    }
                    writer.write_char('*')?;
                    star_added = true;
                    first = false;
                }
                if parameter.is_positional_only() {
                    needs_slash = true;
                } else if needs_slash {
                    if !first {
                        writer.write_str(arg_separator)?;
                    }
                    writer.write_char('/')?;
                    needs_slash = false;
                    first = false;
                }

                // Add comma before parameter if not first
                if !first {
                    writer.write_str(arg_separator)?;
                }

                // Write parameter with range tracking
                let param_name = parameter.display_name();
                writer.write_parameter(
                    &parameter.display_with(self.db, self.settings.singleline()),
                    param_name.as_deref(),
                )?;

                first = false;
            }

            if needs_slash {
                if !first {
                    writer.write_str(arg_separator)?;
                }
                writer.write_char('/')?;
            }
        }

        if multiline {
            writer.write_char('\n')?;
        }
        // Closing parenthesis
        writer.write_char(')')?;

        // Return type
        let return_ty = self.return_ty.unwrap_or_else(Type::unknown);
        writer.write_return_type(&return_ty.display_with(self.db, self.settings.singleline()))?;

        Ok(())
    }
}

impl Display for DisplaySignature<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = SignatureWriter::Formatter(f);
        self.write_signature(&mut writer)
    }
}

/// Writer for building signature strings with different output targets
enum SignatureWriter<'a, 'b> {
    /// Write directly to a formatter (for Display trait)
    Formatter(&'a mut Formatter<'b>),
    /// Build a string with range tracking (for `to_string_parts`)
    Details(SignatureDetailsWriter),
}

/// Writer that builds a string with range tracking
struct SignatureDetailsWriter {
    label: String,
    parameter_ranges: Vec<TextRange>,
    parameter_names: Vec<String>,
}

impl SignatureDetailsWriter {
    fn new() -> Self {
        Self {
            label: String::new(),
            parameter_ranges: Vec::new(),
            parameter_names: Vec::new(),
        }
    }

    fn finish(self) -> SignatureDisplayDetails {
        SignatureDisplayDetails {
            label: self.label,
            parameter_ranges: self.parameter_ranges,
            parameter_names: self.parameter_names,
        }
    }
}

impl SignatureWriter<'_, '_> {
    fn write_char(&mut self, c: char) -> fmt::Result {
        match self {
            SignatureWriter::Formatter(f) => f.write_char(c),
            SignatureWriter::Details(details) => {
                details.label.push(c);
                Ok(())
            }
        }
    }

    fn write_str(&mut self, s: &str) -> fmt::Result {
        match self {
            SignatureWriter::Formatter(f) => f.write_str(s),
            SignatureWriter::Details(details) => {
                details.label.push_str(s);
                Ok(())
            }
        }
    }

    fn write_parameter<T: Display>(&mut self, param: &T, param_name: Option<&str>) -> fmt::Result {
        match self {
            SignatureWriter::Formatter(f) => param.fmt(f),
            SignatureWriter::Details(details) => {
                let param_start = details.label.len();
                let param_display = param.to_string();
                details.label.push_str(&param_display);

                // Use TextSize::try_from for safe conversion, falling back to empty range on overflow
                let start = TextSize::try_from(param_start).unwrap_or_default();
                let length = TextSize::try_from(param_display.len()).unwrap_or_default();
                details.parameter_ranges.push(TextRange::at(start, length));

                // Store the parameter name if available
                if let Some(name) = param_name {
                    details.parameter_names.push(name.to_string());
                } else {
                    details.parameter_names.push(String::new());
                }

                Ok(())
            }
        }
    }

    fn write_return_type<T: Display>(&mut self, return_ty: &T) -> fmt::Result {
        match self {
            SignatureWriter::Formatter(f) => write!(f, " -> {return_ty}"),
            SignatureWriter::Details(details) => {
                let return_display = format!(" -> {return_ty}");
                details.label.push_str(&return_display);
                Ok(())
            }
        }
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

impl<'db> Parameter<'db> {
    fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayParameter<'db> {
        DisplayParameter {
            param: self,
            db,
            settings,
        }
    }
}

struct DisplayParameter<'db> {
    param: &'db Parameter<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayParameter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.param.display_name() {
            f.write_str(&name)?;
            if let Some(annotated_type) = self.param.annotated_type() {
                if self.param.should_annotation_be_displayed() {
                    write!(
                        f,
                        ": {}",
                        annotated_type.display_with(self.db, self.settings.clone())
                    )?;
                }
            }
            // Default value can only be specified if `name` is given.
            if let Some(default_ty) = self.param.default_type() {
                if self.param.annotated_type().is_some() {
                    write!(
                        f,
                        " = {}",
                        default_ty.display_with(self.db, self.settings.clone())
                    )?;
                } else {
                    write!(
                        f,
                        "={}",
                        default_ty.display_with(self.db, self.settings.clone())
                    )?;
                }
            }
        } else if let Some(ty) = self.param.annotated_type() {
            // This case is specifically for the `Callable` signature where name and default value
            // cannot be provided.
            ty.display_with(self.db, self.settings.clone()).fmt(f)?;
        }
        Ok(())
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

impl Display for DisplayOmitted {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let noun = if self.count == 1 {
            self.singular
        } else {
            self.plural
        };
        write!(f, "... omitted {} {}", self.count, noun)
    }
}

impl<'db> UnionType<'db> {
    fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayUnionType<'db> {
        DisplayUnionType {
            db,
            ty: self,
            settings,
        }
    }
}

struct DisplayUnionType<'db> {
    ty: &'db UnionType<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

const UNION_POLICY: TruncationPolicy = TruncationPolicy {
    max: 5,
    max_when_elided: 3,
};

impl Display for DisplayUnionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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

        join.finish()?;

        Ok(())
    }
}

impl fmt::Debug for DisplayUnionType<'_> {
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

impl Display for DisplayLiteralGroup<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Literal[")?;

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

impl<'db> IntersectionType<'db> {
    fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings<'db>,
    ) -> DisplayIntersectionType<'db> {
        DisplayIntersectionType {
            db,
            ty: self,
            settings,
        }
    }
}

struct DisplayIntersectionType<'db> {
    ty: &'db IntersectionType<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayIntersectionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
        f.join(" & ").entries(tys).finish()
    }
}

impl fmt::Debug for DisplayIntersectionType<'_> {
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

impl Display for DisplayMaybeNegatedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str("~")?;
        }
        DisplayMaybeParenthesizedType {
            ty: self.ty,
            db: self.db,
            settings: self.settings.clone(),
        }
        .fmt(f)
    }
}

struct DisplayMaybeParenthesizedType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings<'db>,
}

impl Display for DisplayMaybeParenthesizedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let write_parentheses = |f: &mut Formatter<'_>| {
            write!(
                f,
                "({})",
                self.ty.display_with(self.db, self.settings.clone())
            )
        };
        match self.ty {
            Type::Callable(_)
            | Type::KnownBoundMethod(_)
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::Union(_) => write_parentheses(f),
            Type::Intersection(intersection) if !intersection.has_one_element(self.db) => {
                write_parentheses(f)
            }
            _ => self.ty.display_with(self.db, self.settings.clone()).fmt(f),
        }
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

impl Display for DisplayTypeArray<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.join(", ")
            .entries(
                self.types
                    .iter()
                    .map(|ty| ty.display_with(self.db, self.settings.singleline())),
            )
            .finish()
    }
}

impl<'db> StringLiteralType<'db> {
    fn display_with(
        &'db self,
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

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use ruff_python_ast::name::Name;

    use crate::Db;
    use crate::db::tests::setup_db;
    use crate::place::typing_extensions_symbol;
    use crate::types::{KnownClass, Parameter, Parameters, Signature, Type};

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

    fn display_signature<'db>(
        db: &dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
        return_ty: Option<Type<'db>>,
    ) -> String {
        Signature::new(Parameters::new(parameters), return_ty)
            .display(db)
            .to_string()
    }

    fn display_signature_multiline<'db>(
        db: &dyn Db,
        parameters: impl IntoIterator<Item = Parameter<'db>>,
        return_ty: Option<Type<'db>>,
    ) -> String {
        Signature::new(Parameters::new(parameters), return_ty)
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
            @"(x=int, y: str = str) -> None"
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
            @"(a, b: int, c=Literal[1], d: int = Literal[2], \
                /, e=Literal[3], f: int = Literal[4], *args: object, \
                *, g=Literal[5], h: int = Literal[6], **kwargs: str) -> bytes"
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
            x=int,
            y: str = str
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
            c=Literal[1],
            d: int = Literal[2],
            /,
            e=Literal[3],
            f: int = Literal[4],
            *args: object,
            *,
            g=Literal[5],
            h: int = Literal[6],
            **kwargs: str
        ) -> bytes
        "
        );
    }
}
