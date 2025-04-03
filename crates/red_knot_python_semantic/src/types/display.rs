//! Display implementations for types.

use std::fmt::{self, Display, Formatter, Write};

use ruff_db::display::FormatterJoinExtension;
use ruff_python_ast::str::{Quote, TripleQuotes};
use ruff_python_literal::escape::AsciiEscape;

use crate::types::class_base::ClassBase;
use crate::types::signatures::{Parameter, Parameters, Signature};
use crate::types::{
    ClassLiteralType, InstanceType, IntersectionType, KnownClass, MethodWrapperKind,
    StringLiteralType, Type, UnionType, WrapperDescriptorKind,
};
use crate::Db;
use rustc_hash::FxHashMap;

impl<'db> Type<'db> {
    pub fn display(&self, db: &'db dyn Db) -> DisplayType {
        DisplayType { ty: self, db }
    }
    fn representation(self, db: &'db dyn Db) -> DisplayRepresentation<'db> {
        DisplayRepresentation { db, ty: self }
    }
}

#[derive(Copy, Clone)]
pub struct DisplayType<'db> {
    ty: &'db Type<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let representation = self.ty.representation(self.db);
        match self.ty {
            Type::ClassLiteral(literal) if literal.class().is_known(self.db, KnownClass::Any) => {
                write!(f, "typing.Any")
            }
            Type::IntLiteral(_)
            | Type::BooleanLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::ClassLiteral(_)
            | Type::FunctionLiteral(_) => {
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
}

impl Display for DisplayRepresentation<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self.ty {
            Type::Dynamic(dynamic) => dynamic.fmt(f),
            Type::Never => f.write_str("Never"),
            Type::Instance(InstanceType { class }) => {
                let representation = match class.known(self.db) {
                    Some(KnownClass::NoneType) => "None",
                    Some(KnownClass::NoDefaultType) => "NoDefault",
                    _ => class.name(self.db),
                };
                f.write_str(representation)
            }
            Type::PropertyInstance(_) => f.write_str("property"),
            Type::ModuleLiteral(module) => {
                write!(f, "<module '{}'>", module.module(self.db).name())
            }
            // TODO functions and classes should display using a fully qualified name
            Type::ClassLiteral(ClassLiteralType { class }) => f.write_str(class.name(self.db)),
            Type::SubclassOf(subclass_of_ty) => match subclass_of_ty.subclass_of() {
                // Only show the bare class name here; ClassBase::display would render this as
                // type[<class 'Foo'>] instead of type[Foo].
                ClassBase::Class(class) => write!(f, "type[{}]", class.name(self.db)),
                ClassBase::Dynamic(dynamic) => write!(f, "type[{dynamic}]"),
            },
            Type::KnownInstance(known_instance) => f.write_str(known_instance.repr(self.db)),
            Type::FunctionLiteral(function) => f.write_str(function.name(self.db)),
            Type::Callable(callable) => callable.signature(self.db).display(self.db).fmt(f),
            Type::BoundMethod(bound_method) => {
                write!(
                    f,
                    "<bound method `{method}` of `{instance}`>",
                    method = bound_method.function(self.db).name(self.db),
                    instance = bound_method.self_instance(self.db).display(self.db)
                )
            }
            Type::MethodWrapper(MethodWrapperKind::FunctionTypeDunderGet(function)) => {
                write!(
                    f,
                    "<method-wrapper `__get__` of `{function}`>",
                    function = function.name(self.db)
                )
            }
            Type::MethodWrapper(MethodWrapperKind::PropertyDunderGet(_)) => {
                write!(f, "<method-wrapper `__get__` of `property` object>",)
            }
            Type::MethodWrapper(MethodWrapperKind::PropertyDunderSet(_)) => {
                write!(f, "<method-wrapper `__set__` of `property` object>",)
            }
            Type::WrapperDescriptor(kind) => {
                let (method, object) = match kind {
                    WrapperDescriptorKind::FunctionTypeDunderGet => ("__get__", "function"),
                    WrapperDescriptorKind::PropertyDunderGet => ("__get__", "property"),
                    WrapperDescriptorKind::PropertyDunderSet => ("__set__", "property"),
                };
                write!(f, "<wrapper-descriptor `{method}` of `{object}` objects>")
            }
            Type::Union(union) => union.display(self.db).fmt(f),
            Type::Intersection(intersection) => intersection.display(self.db).fmt(f),
            Type::IntLiteral(n) => n.fmt(f),
            Type::BooleanLiteral(boolean) => f.write_str(if boolean { "True" } else { "False" }),
            Type::StringLiteral(string) => string.display(self.db).fmt(f),
            Type::LiteralString => f.write_str("LiteralString"),
            Type::BytesLiteral(bytes) => {
                let escape =
                    AsciiEscape::with_preferred_quote(bytes.value(self.db).as_ref(), Quote::Double);

                escape.bytes_repr(TripleQuotes::No).write(f)
            }
            Type::SliceLiteral(slice) => {
                f.write_str("slice[")?;
                if let Some(start) = slice.start(self.db) {
                    write!(f, "Literal[{start}]")?;
                } else {
                    f.write_str("None")?;
                }

                f.write_str(", ")?;

                if let Some(stop) = slice.stop(self.db) {
                    write!(f, "Literal[{stop}]")?;
                } else {
                    f.write_str("None")?;
                }

                if let Some(step) = slice.step(self.db) {
                    write!(f, ", Literal[{step}]")?;
                }

                f.write_str("]")
            }
            Type::Tuple(tuple) => {
                f.write_str("tuple[")?;
                let elements = tuple.elements(self.db);
                if elements.is_empty() {
                    f.write_str("()")?;
                } else {
                    elements.display(self.db).fmt(f)?;
                }
                f.write_str("]")
            }
            Type::TypeVar(typevar) => {
                write!(f, "{}", typevar.name(self.db))
            }
            Type::AlwaysTruthy => f.write_str("AlwaysTruthy"),
            Type::AlwaysFalsy => f.write_str("AlwaysFalsy"),
        }
    }
}

impl<'db> Signature<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplaySignature<'db> {
        DisplaySignature {
            parameters: self.parameters(),
            return_ty: self.return_ty,
            db,
        }
    }
}

struct DisplaySignature<'db> {
    parameters: &'db Parameters<'db>,
    return_ty: Option<Type<'db>>,
    db: &'db dyn Db,
}

impl Display for DisplaySignature<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_char('(')?;

        if self.parameters.is_gradual() {
            // We represent gradual form as `...` in the signature, internally the parameters still
            // contain `(*args, **kwargs)` parameters.
            f.write_str("...")?;
        } else {
            let mut star_added = false;
            let mut needs_slash = false;
            let mut join = f.join(", ");

            for parameter in self.parameters.as_slice() {
                if !star_added && parameter.is_keyword_only() {
                    join.entry(&'*');
                    star_added = true;
                }
                if parameter.is_positional_only() {
                    needs_slash = true;
                } else if needs_slash {
                    join.entry(&'/');
                    needs_slash = false;
                }
                join.entry(&parameter.display(self.db));
            }
            if needs_slash {
                join.entry(&'/');
            }
            join.finish()?;
        }

        write!(
            f,
            ") -> {}",
            self.return_ty.unwrap_or(Type::unknown()).display(self.db)
        )?;

        Ok(())
    }
}

impl<'db> Parameter<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayParameter<'db> {
        DisplayParameter { param: self, db }
    }
}

struct DisplayParameter<'db> {
    param: &'db Parameter<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayParameter<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.param.display_name() {
            write!(f, "{name}")?;
            if let Some(annotated_type) = self.param.annotated_type() {
                write!(f, ": {}", annotated_type.display(self.db))?;
            }
            // Default value can only be specified if `name` is given.
            if let Some(default_ty) = self.param.default_type() {
                if self.param.annotated_type().is_some() {
                    write!(f, " = {}", default_ty.display(self.db))?;
                } else {
                    write!(f, "={}", default_ty.display(self.db))?;
                }
            }
        } else if let Some(ty) = self.param.annotated_type() {
            // This case is specifically for the `Callable` signature where name and default value
            // cannot be provided.
            ty.display(self.db).fmt(f)?;
        }
        Ok(())
    }
}

impl<'db> UnionType<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayUnionType<'db> {
        DisplayUnionType { db, ty: self }
    }
}

struct DisplayUnionType<'db> {
    ty: &'db UnionType<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayUnionType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let elements = self.ty.elements(self.db);

        // Group condensed-display types by kind.
        let mut grouped_condensed_kinds = FxHashMap::default();

        for element in elements {
            if let Ok(kind) = CondensedDisplayTypeKind::try_from(*element) {
                grouped_condensed_kinds
                    .entry(kind)
                    .or_insert_with(Vec::new)
                    .push(*element);
            }
        }

        let mut join = f.join(" | ");

        for element in elements {
            if let Ok(kind) = CondensedDisplayTypeKind::try_from(*element) {
                let Some(condensed_kind) = grouped_condensed_kinds.remove(&kind) else {
                    continue;
                };
                join.entry(&DisplayLiteralGroup {
                    literals: condensed_kind,
                    db: self.db,
                });
            } else {
                join.entry(&DisplayMaybeParenthesizedType {
                    ty: *element,
                    db: self.db,
                });
            }
        }

        join.finish()?;

        debug_assert!(grouped_condensed_kinds.is_empty());

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
}

impl Display for DisplayLiteralGroup<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Literal[")?;
        f.join(", ")
            .entries(self.literals.iter().map(|ty| ty.representation(self.db)))
            .finish()?;
        f.write_str("]")
    }
}

/// Enumeration of literal types that are displayed in a "condensed way" inside `Literal` slices.
///
/// For example, `Literal[1] | Literal[2] | Literal["s"]` is displayed as `"Literal[1, 2, "s"]"`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
enum CondensedDisplayTypeKind {
    Class,
    Function,
    LiteralExpression,
}

impl TryFrom<Type<'_>> for CondensedDisplayTypeKind {
    type Error = ();

    fn try_from(value: Type<'_>) -> Result<Self, Self::Error> {
        match value {
            Type::ClassLiteral(_) => Ok(Self::Class),
            Type::FunctionLiteral(_) => Ok(Self::Function),
            Type::IntLiteral(_)
            | Type::StringLiteral(_)
            | Type::BytesLiteral(_)
            | Type::BooleanLiteral(_) => Ok(Self::LiteralExpression),
            _ => Err(()),
        }
    }
}

impl<'db> IntersectionType<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayIntersectionType<'db> {
        DisplayIntersectionType { db, ty: self }
    }
}

struct DisplayIntersectionType<'db> {
    ty: &'db IntersectionType<'db>,
    db: &'db dyn Db,
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
                negated: false,
            })
            .chain(
                self.ty
                    .negative(self.db)
                    .iter()
                    .map(|&ty| DisplayMaybeNegatedType {
                        ty,
                        db: self.db,
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
}

impl Display for DisplayMaybeNegatedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if self.negated {
            f.write_str("~")?;
        }
        DisplayMaybeParenthesizedType {
            ty: self.ty,
            db: self.db,
        }
        .fmt(f)
    }
}

struct DisplayMaybeParenthesizedType<'db> {
    ty: Type<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayMaybeParenthesizedType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        if let Type::Callable(_) | Type::MethodWrapper(_) = self.ty {
            write!(f, "({})", self.ty.display(self.db))
        } else {
            self.ty.display(self.db).fmt(f)
        }
    }
}

pub(crate) trait TypeArrayDisplay<'db> {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray;
}

impl<'db> TypeArrayDisplay<'db> for Box<[Type<'db>]> {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray {
        DisplayTypeArray { types: self, db }
    }
}

impl<'db> TypeArrayDisplay<'db> for Vec<Type<'db>> {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray {
        DisplayTypeArray { types: self, db }
    }
}

impl<'db> TypeArrayDisplay<'db> for [Type<'db>] {
    fn display(&self, db: &'db dyn Db) -> DisplayTypeArray {
        DisplayTypeArray { types: self, db }
    }
}

pub(crate) struct DisplayTypeArray<'b, 'db> {
    types: &'b [Type<'db>],
    db: &'db dyn Db,
}

impl Display for DisplayTypeArray<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.join(", ")
            .entries(self.types.iter().map(|ty| ty.display(self.db)))
            .finish()
    }
}

impl<'db> StringLiteralType<'db> {
    fn display(&'db self, db: &'db dyn Db) -> DisplayStringLiteralType<'db> {
        DisplayStringLiteralType { db, ty: self }
    }
}

struct DisplayStringLiteralType<'db> {
    ty: &'db StringLiteralType<'db>,
    db: &'db dyn Db,
}

impl Display for DisplayStringLiteralType<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let value = self.ty.value(self.db);
        f.write_char('"')?;
        for ch in value.chars() {
            match ch {
                // `escape_debug` will escape even single quotes, which is not necessary for our
                // use case as we are already using double quotes to wrap the string.
                '\'' => f.write_char('\'')?,
                _ => write!(f, "{}", ch.escape_debug())?,
            }
        }
        f.write_char('"')
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::name::Name;

    use crate::db::tests::setup_db;
    use crate::types::{
        KnownClass, Parameter, Parameters, Signature, SliceLiteralType, StringLiteralType, Type,
    };
    use crate::Db;

    #[test]
    fn test_slice_literal_display() {
        let db = setup_db();

        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, None, None, None))
                .display(&db)
                .to_string(),
            "slice[None, None]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, Some(1), None, None))
                .display(&db)
                .to_string(),
            "slice[Literal[1], None]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, None, Some(2), None))
                .display(&db)
                .to_string(),
            "slice[None, Literal[2]]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, Some(1), Some(5), None))
                .display(&db)
                .to_string(),
            "slice[Literal[1], Literal[5]]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, Some(1), Some(5), Some(2)))
                .display(&db)
                .to_string(),
            "slice[Literal[1], Literal[5], Literal[2]]"
        );
        assert_eq!(
            Type::SliceLiteral(SliceLiteralType::new(&db, None, None, Some(2)))
                .display(&db)
                .to_string(),
            "slice[None, None, Literal[2]]"
        );
    }

    #[test]
    fn string_literal_display() {
        let db = setup_db();

        assert_eq!(
            Type::StringLiteral(StringLiteralType::new(&db, r"\n"))
                .display(&db)
                .to_string(),
            r#"Literal["\\n"]"#
        );
        assert_eq!(
            Type::StringLiteral(StringLiteralType::new(&db, "'"))
                .display(&db)
                .to_string(),
            r#"Literal["'"]"#
        );
        assert_eq!(
            Type::StringLiteral(StringLiteralType::new(&db, r#"""#))
                .display(&db)
                .to_string(),
            r#"Literal["\""]"#
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

    #[test]
    fn signature_display() {
        let db = setup_db();

        // Empty parameters with no return type.
        assert_eq!(display_signature(&db, [], None), "() -> Unknown");

        // Empty parameters with a return type.
        assert_eq!(
            display_signature(&db, [], Some(Type::none(&db))),
            "() -> None"
        );

        // Single parameter type (no name) with a return type.
        assert_eq!(
            display_signature(
                &db,
                [Parameter::positional_only(None).with_annotated_type(Type::none(&db))],
                Some(Type::none(&db))
            ),
            "(None, /) -> None"
        );

        // Two parameters where one has annotation and the other doesn't.
        assert_eq!(
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
            "(x=int, y: str = str) -> None"
        );

        // All positional only parameters.
        assert_eq!(
            display_signature(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("x"))),
                    Parameter::positional_only(Some(Name::new_static("y"))),
                ],
                Some(Type::none(&db))
            ),
            "(x, y, /) -> None"
        );

        // Positional-only parameters mixed with non-positional-only parameters.
        assert_eq!(
            display_signature(
                &db,
                [
                    Parameter::positional_only(Some(Name::new_static("x"))),
                    Parameter::positional_or_keyword(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            "(x, /, y) -> None"
        );

        // All keyword-only parameters.
        assert_eq!(
            display_signature(
                &db,
                [
                    Parameter::keyword_only(Name::new_static("x")),
                    Parameter::keyword_only(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            "(*, x, y) -> None"
        );

        // Keyword-only parameters mixed with non-keyword-only parameters.
        assert_eq!(
            display_signature(
                &db,
                [
                    Parameter::positional_or_keyword(Name::new_static("x")),
                    Parameter::keyword_only(Name::new_static("y")),
                ],
                Some(Type::none(&db))
            ),
            "(x, *, y) -> None"
        );

        // A mix of all parameter kinds.
        assert_eq!(
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
            "(a, b: int, c=Literal[1], d: int = Literal[2], \
                /, e=Literal[3], f: int = Literal[4], *args: object, \
                *, g=Literal[5], h: int = Literal[6], **kwargs: str) -> bytes"
        );
    }
}
