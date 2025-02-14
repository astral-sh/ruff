use std::fmt::{Display, Formatter};

use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::name::{QualifiedName, UnqualifiedName};
use ruff_python_ast::{self as ast, Decorator, Expr, StmtClassDef};

use crate::analyze::class::{traverse_base_classes, TraversalContinuation};
use crate::model::SemanticModel;
use crate::{Module, ModuleSource};

#[derive(Debug, Clone, Copy, is_macro::Is)]
pub enum Visibility {
    Public,
    Private,
}

/// Returns `true` if a function is a "static method".
pub fn is_staticmethod(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_builtin_expr(&decorator.expression, "staticmethod"))
}

/// Returns `true` if a function is a "class method".
pub fn is_classmethod(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_builtin_expr(&decorator.expression, "classmethod"))
}

/// Returns `true` if a function definition is an `@overload`.
pub fn is_overload(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(&decorator.expression, "overload"))
}

/// Returns `true` if a function definition is an `@override` (PEP 698).
pub fn is_override(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(&decorator.expression, "override"))
}

/// Returns `true` if a function definition is an abstract method based on its decorators.
pub fn is_abstract(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    AbstractDecoratorKind::from_decorators(decorator_list, semantic).is_some()
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum AbstractDecoratorKind {
    /// `abc.abstractmethod`
    AbstractMethod,
    /// `abc.abstractclassmethod`
    AbstractClassMethod,
    /// `abc.abstractstaticmethod`
    AbstractStaticMethod,
    /// `abc.abstractproperty`
    AbstractProperty,
}

impl AbstractDecoratorKind {
    pub fn from_decorators(decorators: &[Decorator], semantic: &SemanticModel) -> Option<Self> {
        decorators
            .iter()
            .find_map(|decorator| Self::from_decorator(decorator, semantic))
    }
    fn from_decorator(decorator: &Decorator, semantic: &SemanticModel) -> Option<Self> {
        let qualified_name = semantic.resolve_qualified_name(&decorator.expression)?;

        Self::from_name(&qualified_name)
    }

    fn from_name(name: &QualifiedName) -> Option<Self> {
        if let ["abc", abc_method] = name.segments() {
            match *abc_method {
                "abstractmethod" => Some(Self::AbstractMethod),
                "abstractclassmethod" => Some(Self::AbstractClassMethod),
                "abstractstaticmethod" => Some(Self::AbstractStaticMethod),
                "abstractproperty" => Some(Self::AbstractProperty),
                _ => None,
            }
        } else {
            None
        }
    }
}

impl Display for AbstractDecoratorKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AbstractMethod => "abstractmethod",
            Self::AbstractClassMethod => "abstractclassmethod",
            Self::AbstractStaticMethod => "abstractstaticmethod",
            Self::AbstractProperty => "abstractproperty",
        })
    }
}

/// Returns `true` if a function definition is a `@property`.
/// `extra_properties` can be used to check additional non-standard
/// `@property`-like decorators.
pub fn is_property<'a, P, I>(
    decorator_list: &[Decorator],
    extra_properties: P,
    semantic: &SemanticModel,
) -> bool
where
    P: IntoIterator<IntoIter = I>,
    I: Iterator<Item = QualifiedName<'a>> + Clone,
{
    let extra_properties = extra_properties.into_iter();
    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                matches!(
                    qualified_name.segments(),
                    ["" | "builtins" | "enum", "property"]
                        | ["functools", "cached_property"]
                        | ["abc", "abstractproperty"]
                        | ["types", "DynamicClassAttribute"]
                ) || extra_properties
                    .clone()
                    .any(|extra_property| extra_property == qualified_name)
            })
    })
}

/// Returns `true` if a function definition is an `attrs`-like validator based on its decorators.
pub fn is_validator(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list.iter().any(|decorator| {
        let Expr::Attribute(ast::ExprAttribute { value, attr, .. }) = &decorator.expression else {
            return false;
        };

        if attr.as_str() != "validator" {
            return false;
        }

        let Expr::Name(value) = value.as_ref() else {
            return false;
        };

        semantic
            .resolve_name(value)
            .is_some_and(|id| semantic.binding(id).kind.is_assignment())
    })
}

/// Returns `true` if a class is an `final`.
pub fn is_final(decorator_list: &[Decorator], semantic: &SemanticModel) -> bool {
    decorator_list
        .iter()
        .any(|decorator| semantic.match_typing_expr(&decorator.expression, "final"))
}

/// Returns `true` if a function is a "magic method".
pub fn is_magic(name: &str) -> bool {
    name.starts_with("__") && name.ends_with("__")
}

/// Returns `true` if a function is an `__init__`.
pub fn is_init(name: &str) -> bool {
    name == "__init__"
}

/// Returns `true` if a function is a `__new__`.
pub fn is_new(name: &str) -> bool {
    name == "__new__"
}

/// Returns `true` if a function is a `__call__`.
pub fn is_call(name: &str) -> bool {
    name == "__call__"
}

/// Returns `true` if a function is a test one.
pub fn is_test(name: &str) -> bool {
    name == "runTest" || name.starts_with("test")
}

/// Returns `true` if a module name indicates public visibility.
fn is_public_module(module_name: &str) -> bool {
    !module_name.starts_with('_') || is_magic(module_name)
}

/// Returns `true` if a module name indicates private visibility.
fn is_private_module(module_name: &str) -> bool {
    !is_public_module(module_name)
}

/// Return the stem of a module name (everything preceding the last dot).
fn stem(path: &str) -> &str {
    if let Some(index) = path.rfind('.') {
        &path[..index]
    } else {
        path
    }
}

/// Infer the [`Visibility`] of a module from its path.
pub(crate) fn module_visibility(module: &Module) -> Visibility {
    match &module.source {
        ModuleSource::Path(path) => {
            if path.iter().any(|m| is_private_module(m)) {
                return Visibility::Private;
            }
        }
        ModuleSource::File(path) => {
            // Check to see if the filename itself indicates private visibility.
            // Ex) `_foo.py` (but not `__init__.py`)
            let mut components = path.iter().rev();
            if let Some(filename) = components.next() {
                let module_name = filename.to_string_lossy();
                let module_name = stem(&module_name);
                if is_private_module(module_name) {
                    return Visibility::Private;
                }
            }
        }
    }
    Visibility::Public
}

/// Infer the [`Visibility`] of a function from its name.
pub(crate) fn function_visibility(function: &ast::StmtFunctionDef) -> Visibility {
    if function.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

/// Infer the [`Visibility`] of a method from its name and decorators.
pub fn method_visibility(function: &ast::StmtFunctionDef) -> Visibility {
    // Is this a setter or deleter?
    if function.decorator_list.iter().any(|decorator| {
        UnqualifiedName::from_expr(&decorator.expression).is_some_and(|name| {
            name.segments() == [function.name.as_str(), "setter"]
                || name.segments() == [function.name.as_str(), "deleter"]
        })
    }) {
        return Visibility::Private;
    }

    // Is the method non-private?
    if !function.name.starts_with('_') {
        return Visibility::Public;
    }

    // Is this a magic method?
    if is_magic(&function.name) {
        return Visibility::Public;
    }

    Visibility::Private
}

/// Infer the [`Visibility`] of a class from its name.
pub(crate) fn class_visibility(class: &ast::StmtClassDef) -> Visibility {
    if class.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

pub fn is_abc_abc(base: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(base) else {
        return false;
    };

    matches!(qualified_name.segments(), ["abc", "ABC"])
}

pub fn is_abc_abcmeta(base: &Expr, semantic: &SemanticModel) -> bool {
    let Some(qualified_name) = semantic.resolve_qualified_name(base) else {
        return false;
    };

    matches!(qualified_name.segments(), ["abc", "ABCMeta"])
}

#[derive(Debug)]
enum InspectableClass<'a> {
    Imported(QualifiedName<'a>),
    ClassDef(&'a StmtClassDef),
}

impl<'a> InspectableClass<'a> {
    fn from(expr: &'a Expr, semantic: &'a SemanticModel) -> Option<Self> {
        let expr = map_subscript(expr);

        let name = expr.as_name_expr()?;
        let binding_id = semantic.only_binding(name)?;
        let binding = semantic.binding(binding_id);

        if let Some(class_def) = binding.statement(semantic)?.as_class_def_stmt() {
            return Some(Self::ClassDef(class_def));
        };

        let qualified_name = semantic.resolve_qualified_name(expr)?;
        Some(Self::Imported(qualified_name))
    }

    fn is_abc_abc(&self) -> bool {
        match self {
            Self::ClassDef(_) => false,
            Self::Imported(qualified_name) => matches!(qualified_name.segments(), ["abc", "ABC"]),
        }
    }

    fn is_abc_abcmeta(&self) -> bool {
        match self {
            Self::ClassDef(_) => false,
            Self::Imported(qualified_name) => {
                matches!(qualified_name.segments(), ["abc", "ABCMeta"])
            }
        }
    }
}

/// Whether a given class is likely to be an abstract base class.
///
/// See [`ABCLikeliness::from`] for the algorithm.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum ABCLikeliness {
    No,
    LikelyNo,
    Unknown,
    LikelyYes,
    Yes,
}

impl ABCLikeliness {
    /// Whether `self` is neither [`Self::No`] nor [`Self::LikelyNo`].
    pub fn might_be_abstract(self) -> bool {
        self > Self::LikelyNo
    }
}

impl ABCLikeliness {
    /// Given a class `A` which may or may not have a metaclass `M`:
    ///
    /// * If there are no explicit base classes,
    ///   returns [`Self::No`].
    ///
    /// * If `M` is `ABCMeta`, or if `A` inherits from `ABC` directly,
    ///   returns [`Self::Yes`].
    ///   This relies on the assumption that, by doing so,
    ///   the user wanted `A` to be abstract.
    ///
    /// * If `M` inherits from `ABCMeta`, returns [`Self::LikelyYes`].
    ///
    /// * For each *indirect* base class `B` of `A`, returns [`Self::LikelyYes`] if:
    ///   * `B` is `ABC`, or
    ///   * `B`'s metaclass is `ABCMeta` or a subclass thereof.
    ///
    /// * Otherwise, returns [`Self::LikelyNo`].
    ///
    /// From step 3 onwards, if any base/metaclass is found
    /// not to be inspectable, returns [`Self::Unknown`].
    pub fn from(class_def: &StmtClassDef, semantic: &SemanticModel) -> Self {
        let Some(arguments) = class_def.arguments.as_ref() else {
            return Self::No;
        };

        if arguments.is_empty() {
            return Self::No;
        }

        if let Some(metaclass) = arguments.find_keyword("metaclass") {
            if let Some(likeliness) = Self::from_metaclass(&metaclass.value, semantic, true) {
                return likeliness;
            }
        };

        for base_expr in class_def.bases() {
            let Some(base) = InspectableClass::from(base_expr, semantic) else {
                return Self::Unknown;
            };

            if base.is_abc_abc() {
                return Self::Yes;
            }

            let InspectableClass::ClassDef(base_class_def) = base else {
                continue;
            };

            let Some(arguments) = base_class_def.arguments.as_ref() else {
                continue;
            };

            if arguments.is_empty() {
                continue;
            }

            if let Some(metaclass) = arguments.find_keyword("metaclass") {
                if let Some(likeliness) = Self::from_metaclass(&metaclass.value, semantic, false) {
                    return likeliness;
                }
            };
        }

        let mut likeliness = Self::LikelyNo;

        traverse_base_classes(class_def, semantic, &mut |expr| {
            if class_def.bases().contains(expr) {
                return TraversalContinuation::Continue;
            }

            let Some(base) = InspectableClass::from(expr, semantic) else {
                likeliness = Self::Unknown;
                return TraversalContinuation::Stop;
            };

            if base.is_abc_abc() {
                likeliness = Self::LikelyYes;
                return TraversalContinuation::Stop;
            }

            let InspectableClass::ClassDef(base_class_def) = base else {
                return TraversalContinuation::Continue;
            };

            let Some(arguments) = base_class_def.arguments.as_ref() else {
                return TraversalContinuation::Continue;
            };

            if arguments.is_empty() {
                return TraversalContinuation::Continue;
            }

            let Some(metaclass) = arguments.find_keyword("metaclass") else {
                return TraversalContinuation::Continue;
            };

            if let Some(new_likeliness) = Self::from_metaclass(&metaclass.value, semantic, false) {
                likeliness = new_likeliness;
                return TraversalContinuation::Stop;
            }

            TraversalContinuation::Continue
        });

        likeliness
    }

    fn from_metaclass(metaclass: &Expr, semantic: &SemanticModel, direct: bool) -> Option<Self> {
        let Some(metaclass) = InspectableClass::from(metaclass, semantic) else {
            return Some(Self::Unknown);
        };

        if metaclass.is_abc_abcmeta() {
            return if direct {
                Some(Self::Yes)
            } else {
                Some(Self::LikelyYes)
            };
        }

        let InspectableClass::ClassDef(metaclass_class_def) = metaclass else {
            return Some(Self::Unknown);
        };

        for base_expr in metaclass_class_def.bases() {
            let Some(base) = InspectableClass::from(base_expr, semantic) else {
                return Some(Self::Unknown);
            };

            if base.is_abc_abcmeta() {
                return Some(Self::LikelyYes);
            }
        }

        let mut likeliness = None;

        traverse_base_classes(metaclass_class_def, semantic, &mut |expr| {
            let Some(base) = InspectableClass::from(expr, semantic) else {
                likeliness = Some(Self::Unknown);
                return TraversalContinuation::Stop;
            };

            if base.is_abc_abcmeta() {
                likeliness = Some(Self::LikelyYes);
                return TraversalContinuation::Stop;
            }

            TraversalContinuation::Continue
        });

        likeliness
    }
}
