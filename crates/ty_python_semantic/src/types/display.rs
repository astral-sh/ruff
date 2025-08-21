//! Display implementations for types.

use std::fmt::{self, Display, Formatter, Write};

use ruff_db::display::FormatterJoinExtension;
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_literal::escape::AsciiEscape;
use ruff_text_size::{TextRange, TextSize};

use crate::Db;
use crate::types::class::{ClassLiteral, ClassType, GenericAlias};
use crate::types::function::{FunctionType, OverloadLiteral};
use crate::types::generics::{GenericContext, Specialization};
use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};
use crate::types::tuple::TupleSpec;
use crate::types::{
    CallableType, IntersectionType, KnownClass, MethodWrapperKind, Protocol, StringLiteralType,
    SubclassOfInner, Type, UnionType, WrapperDescriptorKind,
};

/// Settings for displaying types and signatures
#[derive(Debug, Copy, Clone, Default)]
pub struct DisplaySettings {
    /// Whether rendering can be multiline
    pub multiline: bool,
}

impl DisplaySettings {
    #[must_use]
    pub fn multiline(self) -> Self {
        Self { multiline: true }
    }

    #[must_use]
    pub fn singleline(self) -> Self {
        Self { multiline: false }
    }
}

impl<'db> Type<'db> {
    pub fn display(&self, db: &'db dyn Db) -> DisplayType<'_> {
        DisplayType {
            ty: self,
            settings: DisplaySettings::default(),
            db,
        }
    }
    pub fn display_with(&self, db: &'db dyn Db, settings: DisplaySettings) -> DisplayType<'_> {
        DisplayType {
            ty: self,
            db,
            settings,
        }
    }
    fn representation(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings,
    ) -> DisplayRepresentation<'db> {
        DisplayRepresentation {
            db,
            ty: self,
            settings,
        }
    }
}

#[derive(Copy, Clone)]
pub struct DisplayType<'db> {
    ty: &'db Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings,
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let representation = self.ty.representation(self.db, self.settings);
        match self.ty {
            Type::ClassLiteral(literal) if literal.is_known(self.db, KnownClass::Any) => {
                write!(f, "typing.Any")
            }
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

/// Writes the string representation of a type, which is the value displayed either as
/// `Literal[<repr>]` or `Literal[<repr1>, <repr2>]` for literal types or as `<repr>` for
/// non literals
struct DisplayRepresentation<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings,
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
                        .display_with(self.db, self.settings)
                        .fmt(f),
                    (ClassType::NonGeneric(class), _) => f.write_str(class.name(self.db)),
                    (ClassType::Generic(alias), _) => alias.display_with(self.db, self.settings).fmt(f),
                }
            }
            Type::ProtocolInstance(protocol) => match protocol.inner {
                Protocol::FromClass(ClassType::NonGeneric(class)) => {
                    f.write_str(class.name(self.db))
                }
                Protocol::FromClass(ClassType::Generic(alias)) => {
                    alias.display_with(self.db, self.settings).fmt(f)
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
            Type::ClassLiteral(class) => {
                write!(f, "<class '{}'>", class.name(self.db))
            }
            Type::GenericAlias(generic) => write!(
                f,
                "<class '{}'>",
                generic.display_with(self.db, self.settings.singleline())
            ),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                SubclassOfInner::Class(ClassType::NonGeneric(class)) => {
                    write!(f, "type[{}]", class.name(self.db))
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
            Type::FunctionLiteral(function) => function.display_with(self.db, self.settings).fmt(f),
            Type::Callable(callable) => callable.display_with(self.db, self.settings).fmt(f),
            Type::BoundMethod(bound_method) => {
                let function = bound_method.function(self.db);
                let self_ty = bound_method.self_instance(self.db);
                let typing_self_ty = bound_method.typing_self_type(self.db);

                match function.signature(self.db).overloads.as_slice() {
                    [signature] => {
                        let type_parameters = DisplayOptionalGenericContext {
                            generic_context: signature.generic_context.as_ref(),
                            db: self.db,
                            settings: self.settings,
                        };

                        write!(
                            f,
                            "bound method {instance}.{method}{type_parameters}{signature}",
                            method = function.name(self.db),
                            instance = self_ty.display_with(self.db, self.settings.singleline()),
                            type_parameters = type_parameters,
                            signature = signature
                                .bind_self(self.db, Some(typing_self_ty))
                                .display_with(self.db, self.settings)
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
                                    .display_with(self.db, self.settings),
                            );
                        }
                        if !self.settings.multiline {
                            f.write_str("]")?;
                        }
                        Ok(())
                    }
                }
            }
            Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)) => {
                write!(
                    f,
                    "<method-wrapper `__get__` of `{function}`>",
                    function = function.name(self.db),
                )
            }
            Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderCall(function)) => {
                write!(
                    f,
                    "<method-wrapper `__call__` of `{function}`>",
                    function = function.name(self.db),
                )
            }
            Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(_)) => {
                write!(f, "<method-wrapper `__get__` of `property` object>",)
            }
            Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(_)) => {
                write!(f, "<method-wrapper `__set__` of `property` object>",)
            }
            Type::MethodWrapper(MethodWrapperKind::StrStartswith(_)) => {
                write!(f, "<method-wrapper `startswith` of `str` object>",)
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
            Type::Union(union) => union.display_with(self.db, self.settings).fmt(f),
            Type::Intersection(intersection) => {
                intersection.display_with(self.db, self.settings).fmt(f)
            }
            Type::IntLiteral(n) => n.fmt(f),
            Type::BooleanLiteral(boolean) => f.write_str(if boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => string.display_with(self.db, self.settings).fmt(f),
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape = AsciiEscape::with_preferred_quote(bytes.value(self.db), Quote::Double);

                escape.bytes_repr(TripleQuotes::No).write(f)
            }
            Type::EnumLiteral(enum_literal) => {
                write!(
                    f,
                    "{enum_class}.{name}",
                    enum_class = enum_literal.enum_class(self.db).name(self.db),
                    name = enum_literal.name(self.db),
                )
            }
            Type::NonInferableTypeVar(bound_typevar) | Type::TypeVar(bound_typevar) => {
                f.write_str(bound_typevar.typevar(self.db).name(self.db))?;
                if let Some(binding_context) = bound_typevar.binding_context(self.db).name(self.db)
                {
                    write!(f, "@{binding_context}")?;
                }
                Ok(())
            }
            Type::AlwaysTruthy => f.write_str("AlwaysTruthy"),
            Type::AlwaysFalsy => f.write_str("AlwaysFalsy"),
            Type::BoundSuper(bound_super) => {
                write!(
                    f,
                    "<super: {pivot}, {owner}>",
                    pivot = Type::from(bound_super.pivot_class(self.db))
                        .display_with(self.db, self.settings.singleline()),
                    owner = bound_super
                        .owner(self.db)
                        .into_type()
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
            Type::TypedDict(typed_dict) => f.write_str(typed_dict.defining_class.name(self.db)),
            Type::TypeAlias(alias) => f.write_str(alias.name(self.db)),
        }
    }
}

impl<'db> TupleSpec<'db> {
    pub(crate) fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings,
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
    settings: DisplaySettings,
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
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayOverloadLiteral<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let signature = self.literal.signature(self.db, None);
        let type_parameters = DisplayOptionalGenericContext {
            generic_context: signature.generic_context.as_ref(),
            db: self.db,
            settings: self.settings,
        };

        write!(
            f,
            "def {name}{type_parameters}{signature}",
            name = self.literal.name(self.db),
            type_parameters = type_parameters,
            signature = signature.display_with(self.db, self.settings)
        )
    }
}

impl<'db> FunctionType<'db> {
    pub(crate) fn display_with(
        self,
        db: &'db dyn Db,
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayFunctionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let signature = self.ty.signature(self.db);

        match signature.overloads.as_slice() {
            [signature] => {
                let type_parameters = DisplayOptionalGenericContext {
                    generic_context: signature.generic_context.as_ref(),
                    db: self.db,
                    settings: self.settings,
                };

                write!(
                    f,
                    "def {name}{type_parameters}{signature}",
                    name = self.ty.name(self.db),
                    type_parameters = type_parameters,
                    signature = signature.display_with(self.db, self.settings)
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
                    join.entry(&signature.display_with(self.db, self.settings));
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
    pub(crate) fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayGenericAlias<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(tuple) = self.specialization.tuple(self.db) {
            tuple.display_with(self.db, self.settings).fmt(f)
        } else {
            write!(
                f,
                "{origin}{specialization}",
                origin = self.origin.name(self.db),
                specialization = self.specialization.display_short(
                    self.db,
                    TupleSpecialization::from_class(self.db, self.origin)
                ),
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
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayOptionalGenericContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(generic_context) = self.generic_context {
            DisplayGenericContext {
                generic_context,
                db: self.db,
                settings: self.settings,
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
    settings: DisplaySettings,
}

impl Display for DisplayGenericContext<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let variables = self.generic_context.variables(self.db);

        let non_implicit_variables: Vec<_> = variables
            .iter()
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
    /// Renders the specialization as it would appear in a subscript expression, e.g. `[int, str]`.
    pub fn display_short(
        &'db self,
        db: &'db dyn Db,
        tuple_specialization: TupleSpecialization,
    ) -> DisplaySpecialization<'db> {
        DisplaySpecialization {
            types: self.types(db),
            db,
            tuple_specialization,
            settings: DisplaySettings::default(),
        }
    }
}

pub struct DisplaySpecialization<'db> {
    types: &'db [Type<'db>],
    db: &'db dyn Db,
    tuple_specialization: TupleSpecialization,
    settings: DisplaySettings,
}

impl Display for DisplaySpecialization<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_char('[')?;
        for (idx, ty) in self.types.iter().enumerate() {
            if idx > 0 {
                f.write_str(", ")?;
            }
            ty.display_with(self.db, self.settings).fmt(f)?;
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
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayCallableType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.signatures.overloads.as_slice() {
            [signature] => signature.display_with(self.db, self.settings).fmt(f),
            signatures => {
                // TODO: How to display overloads?
                if !self.settings.multiline {
                    f.write_str("Overload[")?;
                }
                let separator = if self.settings.multiline { "\n" } else { ", " };
                let mut join = f.join(separator);
                for signature in signatures {
                    join.entry(&signature.display_with(self.db, self.settings));
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
        settings: DisplaySettings,
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
    settings: DisplaySettings,
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
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayParameter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.param.display_name() {
            f.write_str(&name)?;
            if let Some(annotated_type) = self.param.annotated_type() {
                write!(
                    f,
                    ": {}",
                    annotated_type.display_with(self.db, self.settings)
                )?;
            }
            // Default value can only be specified if `name` is given.
            if let Some(default_ty) = self.param.default_type() {
                if self.param.annotated_type().is_some() {
                    write!(f, " = {}", default_ty.display_with(self.db, self.settings))?;
                } else {
                    write!(f, "={}", default_ty.display_with(self.db, self.settings))?;
                }
            }
        } else if let Some(ty) = self.param.annotated_type() {
            // This case is specifically for the `Callable` signature where name and default value
            // cannot be provided.
            ty.display_with(self.db, self.settings).fmt(f)?;
        }
        Ok(())
    }
}

impl<'db> UnionType<'db> {
    fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings,
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
    settings: DisplaySettings,
}

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

        let mut join = f.join(" | ");

        let mut condensed_types = Some(condensed_types);
        for element in elements {
            if is_condensable(*element) {
                if let Some(condensed_types) = condensed_types.take() {
                    join.entry(&DisplayLiteralGroup {
                        literals: condensed_types,
                        db: self.db,
                        settings: self.settings.singleline(),
                    });
                }
            } else {
                join.entry(&DisplayMaybeParenthesizedType {
                    ty: *element,
                    db: self.db,
                    settings: self.settings.singleline(),
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
    settings: DisplaySettings,
}

impl Display for DisplayLiteralGroup<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Literal[")?;
        f.join(", ")
            .entries(
                self.literals
                    .iter()
                    .map(|ty| ty.representation(self.db, self.settings.singleline())),
            )
            .finish()?;
        f.write_str("]")
    }
}

impl<'db> IntersectionType<'db> {
    fn display_with(
        &'db self,
        db: &'db dyn Db,
        settings: DisplaySettings,
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
    settings: DisplaySettings,
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
    settings: DisplaySettings,
}

impl Display for DisplayMaybeNegatedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str("~")?;
        }
        DisplayMaybeParenthesizedType {
            ty: self.ty,
            db: self.db,
            settings: self.settings,
        }
        .fmt(f)
    }
}

struct DisplayMaybeParenthesizedType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
    settings: DisplaySettings,
}

impl Display for DisplayMaybeParenthesizedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let write_parentheses =
            |f: &mut Formatter<'_>| write!(f, "({})", self.ty.display_with(self.db, self.settings));
        match self.ty {
            Type::Callable(_)
            | Type::MethodWrapper(_)
            | Type::FunctionLiteral(_)
            | Type::BoundMethod(_)
            | Type::Union(_) => write_parentheses(f),
            Type::Intersection(intersection) if !intersection.has_one_element(self.db) => {
                write_parentheses(f)
            }
            _ => self.ty.display_with(self.db, self.settings).fmt(f),
        }
    }
}

pub(crate) trait TypeArrayDisplay<'db> {
    fn display_with(&self, db: &'db dyn Db, settings: DisplaySettings)
    -> DisplayTypeArray<'_, 'db>;
}

impl<'db> TypeArrayDisplay<'db> for Box<[Type<'db>]> {
    fn display_with(
        &self,
        db: &'db dyn Db,
        settings: DisplaySettings,
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
        settings: DisplaySettings,
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
        settings: DisplaySettings,
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
    settings: DisplaySettings,
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
        settings: DisplaySettings,
    ) -> DisplayStringLiteralType<'db> {
        display_quoted_string(self.value(db), settings)
    }
}

fn display_quoted_string(string: &str, settings: DisplaySettings) -> DisplayStringLiteralType<'_> {
    DisplayStringLiteralType { string, settings }
}

struct DisplayStringLiteralType<'db> {
    string: &'db str,
    #[expect(dead_code)]
    settings: DisplaySettings,
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
            .ignore_possibly_unbound()
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
                        .with_annotated_type(Type::object(&db)),
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
                        .with_annotated_type(Type::object(&db)),
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
