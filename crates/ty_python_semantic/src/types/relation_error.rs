use crate::Db;
use crate::types::relation::TypeRelation;
/// This module defines a tree structure for collecting contextual information about type relation errors
/// ("why is this complex type not assignable to that other complex type?").
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ruff_python_ast::name::Name;

use crate::types::context::LintDiagnosticGuard;
use crate::types::tuple::TupleLength;
use crate::types::{DisplaySettings, Type, TypedDictType};
use crate::{FxOrderSet, ProgramEnvironment};

/// Identifies a parameter, either by name or by position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParameterDescription {
    Named(Name),
    /// 0-based index
    Index(usize),
}

impl ParameterDescription {
    pub(crate) fn new(index: usize, name: Option<&Name>) -> Self {
        match name {
            Some(name) => Self::Named(name.clone()),
            None => Self::Index(index),
        }
    }
}

impl std::fmt::Display for ParameterDescription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Named(name) => write!(f, "parameter `{name}`"),
            Self::Index(0) => f.write_str("the first parameter"),
            Self::Index(1) => f.write_str("the second parameter"),
            Self::Index(2) => f.write_str("the third parameter"),
            Self::Index(n) => write!(f, "parameter {}", n + 1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorContext<'db> {
    /// No additional context is available.
    Empty,
    NotAllUnionElementsAssignable {
        element: Type<'db>,
        union: Type<'db>,
        target: Type<'db>,
    },
    NotAssignableToAnyUnionElement {
        source: Type<'db>,
        union: Type<'db>,
    },
    NotAssignableToNOtherUnionElements {
        n: usize,
    },
    NotAssignableToIntersectionElement {
        source: Type<'db>,
        element: Type<'db>,
        intersection: Type<'db>,
    },
    NoIntersectionElementAssignableToTarget {
        intersection: Type<'db>,
        target: Type<'db>,
    },
    TypedDictFieldMissing {
        field_name: Name,
        source: TypedDictType<'db>,
    },
    TypedDictFieldNotRequiredInSource {
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
        field_name: Name,
    },
    TypedDictFieldNotRequiredAndMutableInTarget {
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
        field_name: Name,
    },
    TypedDictFieldReadOnlyInSource {
        field_name: Name,
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
    },
    TypedDictFieldIncompatible {
        field_name: Name,
        source: TypedDictType<'db>,
        target: TypedDictType<'db>,
        source_field: Type<'db>,
        target_field: Type<'db>,
    },
    TypedDictNotAssignableToDict(TypedDictType<'db>),
    OpenTypedDictNotAssignableToMapping {
        source: TypedDictType<'db>,
        target: Type<'db>,
    },
    IncompatibleReturnTypes {
        source: Type<'db>,
        target: Type<'db>,
    },
    IncompatibleParameterTypes {
        source: Type<'db>,
        target: Type<'db>,
        parameter: ParameterDescription,
    },
    InferredCallableType {
        source: Type<'db>,
        callable: Type<'db>,
    },
    ExtraRequiredParameter {
        parameter: ParameterDescription,
    },
    MissingParameter {
        parameter: ParameterDescription,
    },
    RequiredParameterMustHaveDefault {
        parameter: ParameterDescription,
    },
    MissingVariadicPositionalParameter,
    MissingVariadicKeywordParameter,
    TopCallableAssignedToNonTop {
        return_type: Type<'db>,
    },
    ParameterNameMismatch {
        source_name: Name,
        target_name: Name,
    },
    ParameterMustAcceptKeywordArguments {
        source_name: Option<Name>,
        target_name: Name,
    },
    ParameterMustAcceptPositionalArguments {
        name: Name,
    },
    TupleLengthMismatch {
        source_len: usize,
        target_len: TupleLength,
    },
    TupleElementNotCompatible {
        source: Type<'db>,
        target: Type<'db>,
        element_index: usize,
        element_count: usize,
    },
    TypeNotCompatibleWithProtocol {
        ty: Type<'db>,
        protocol: Type<'db>,
    },
    ProtocolMemberNotDefined {
        member_name: Name,
        ty: Type<'db>,
    },
    ProtocolMemberClassVarMismatch {
        member_name: Name,
        ty: Type<'db>,
    },
    ProtocolSpecialMethodNotDefinedOnMetaType,
    ProtocolMemberIncompatible {
        member_name: Name,
    },
    ProtocolMemberReadTypeIncompatible {
        source: Type<'db>,
        target: Type<'db>,
    },
    ProtocolMemberNotWritable,
    ProtocolMemberWriteTypeIncompatible {
        target: Type<'db>,
    },
}

impl<'db> ErrorContext<'db> {
    fn render(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        relation: TypeRelation,
        help_messages: &mut FxOrderSet<HelpMessages>,
    ) -> Option<String> {
        let typed_dict_name = |typed_dict: &TypedDictType<'db>| match typed_dict {
            TypedDictType::Class(class) => format!("TypedDict `{}`", class.name(db)),
            TypedDictType::Synthesized(_) => {
                Type::TypedDict(*typed_dict).display(db, env).to_string()
            }
        };

        Some(match self {
            Self::Empty => {
                return None;
            }
            Self::NotAllUnionElementsAssignable {
                element,
                union,
                target,
            } => {
                let settings = DisplaySettings::from_possibly_ambiguous_types(
                    db,
                    env,
                    [*element, *union, *target],
                );
                format!(
                    "element `{}` of union `{}` is not {} `{}`",
                    element.display_with(db, env, settings.clone()),
                    union.display_with(db, env, settings.expand_numeric_tower_unions()),
                    relation.description(),
                    target.display_with(db, env, settings),
                )
            }
            Self::NotAssignableToAnyUnionElement { source, union } => {
                let settings =
                    DisplaySettings::from_possibly_ambiguous_types(db, env, [*source, *union]);
                format!(
                    "type `{}` is not {} any element of the union `{}`",
                    source.display_with(db, env, settings.clone()),
                    relation.description(),
                    union.display_with(db, env, settings.expand_numeric_tower_unions()),
                )
            }
            Self::NotAssignableToNOtherUnionElements { n } => format!(
                "... omitted {n} union element{} without additional context",
                if *n == 1 { "" } else { "s" }
            ),
            Self::NotAssignableToIntersectionElement {
                source,
                element,
                intersection,
            } => format!(
                "type `{}` is not {} element `{}` of intersection `{}`",
                source.display(db, env),
                relation.description(),
                element.display(db, env),
                intersection.display(db, env),
            ),
            Self::NoIntersectionElementAssignableToTarget {
                intersection,
                target,
            } => format!(
                "no element of intersection `{}` is {} `{}`",
                intersection.display(db, env),
                relation.description(),
                target.display(db, env),
            ),
            Self::TypedDictFieldMissing { field_name, source } => {
                format!(
                    "required field \"{field_name}\" is not present in source {source}",
                    source = typed_dict_name(source)
                )
            }
            Self::TypedDictFieldNotRequiredInSource {
                field_name,
                source,
                target,
            } => {
                format!(
                    "field \"{field_name}\" is required in {target} but not required in {source}",
                    source = typed_dict_name(source),
                    target = typed_dict_name(target)
                )
            }
            Self::TypedDictFieldNotRequiredAndMutableInTarget {
                field_name,
                source,
                target,
            } => {
                help_messages.insert(HelpMessages::RequiredFieldCouldBeRemoved);
                format!(
                    "field \"{field_name}\" is required in {source} but not required and mutable in {target}",
                    source = typed_dict_name(source),
                    target = typed_dict_name(target)
                )
            }
            Self::TypedDictFieldReadOnlyInSource {
                field_name,
                source,
                target,
            } => {
                format!(
                    "field \"{field_name}\" is read-only in {source} but mutable in {target}",
                    source = typed_dict_name(source),
                    target = typed_dict_name(target)
                )
            }
            Self::TypedDictFieldIncompatible {
                field_name,
                source,
                target,
                source_field,
                target_field,
            } => format!(
                "field \"{field_name}\" on {source} has type `{source_field}` which is not {relation} type `{target_field}` expected by {target}",
                source = typed_dict_name(source),
                target = typed_dict_name(target),
                relation = relation.description(),
                source_field = source_field.display(db, env),
                target_field = target_field.display(db, env),
            ),
            Self::TypedDictNotAssignableToDict(typed_dict) => {
                help_messages.insert(HelpMessages::TypedDictNotAssignableToDict(relation));
                help_messages.insert(HelpMessages::ConsiderUsingMappingInsteadOfDict);

                format!(
                    "{source} is not {relation} `dict`",
                    source = typed_dict_name(typed_dict),
                    relation = relation.description()
                )
            }
            Self::OpenTypedDictNotAssignableToMapping { source, target } => {
                let name = source.defining_class().map(|class| class.name(db));
                help_messages.insert(HelpMessages::OpenTypedDictNotAssignableToMapping {
                    typed_dict_name: name.cloned(),
                    relation,
                });
                help_messages.insert(HelpMessages::ExplainOpenTypedDictUnsoundness {
                    typed_dict_name: name.cloned(),
                });

                format!(
                    "{source} is not {relation} `{target}`",
                    source = typed_dict_name(source),
                    relation = relation.description(),
                    target = target.display(db, env)
                )
            }
            Self::IncompatibleReturnTypes { source, target } => format!(
                "incompatible return types: `{source}` is not {relation} `{target}`",
                source = source.display(db, env),
                relation = relation.description(),
                target = target.display(db, env),
            ),
            Self::IncompatibleParameterTypes {
                source,
                target,
                parameter,
            } => {
                // reversed order due to contravariance of parameter types
                format!(
                    "{parameter} has an incompatible type: `{target}` is not {relation} `{source}`",
                    source = source.display(db, env),
                    relation = relation.description(),
                    target = target.display(db, env),
                )
            }
            Self::InferredCallableType { source, callable } => format!(
                "type `{}` has inferred callable type `{}`",
                source.display(db, env),
                callable.display(db, env),
            ),
            Self::ExtraRequiredParameter { parameter } => match parameter {
                ParameterDescription::Named(name) => {
                    help_messages.insert(HelpMessages::ConsiderAddingADefaultValue {
                        parameter_name: Some(name.clone()),
                    });
                    format!("unexpected extra parameter `{name}`")
                }
                ParameterDescription::Index(_) => {
                    help_messages.insert(HelpMessages::ConsiderAddingADefaultValue {
                        parameter_name: None,
                    });
                    "unexpected extra parameter".to_string()
                }
            },
            Self::MissingParameter { parameter } => format!("{parameter} is missing"),
            Self::RequiredParameterMustHaveDefault { parameter } => {
                format!("{parameter} must have a default value")
            }
            Self::MissingVariadicPositionalParameter => {
                "the signature must accept arbitrary positional arguments".to_string()
            }
            Self::MissingVariadicKeywordParameter => {
                "the signature must accept arbitrary keyword arguments".to_string()
            }
            Self::TopCallableAssignedToNonTop { return_type } => {
                help_messages.insert(HelpMessages::TopCallableExplanation);
                format!(
                    "Object of type `Top[(...) -> {}]` is not safe to call; its signature is not known",
                    return_type.display(db, env)
                )
            }
            Self::ParameterNameMismatch {
                source_name,
                target_name,
            } => format!(
                "the parameter named `{source_name}` does not match `{target_name}` (and can be used as a keyword parameter)",
            ),
            Self::ParameterMustAcceptKeywordArguments {
                source_name,
                target_name,
            } => {
                if let Some(source_name) = source_name {
                    format!(
                        "parameter `{source_name}` is positional-only but must also accept keyword arguments",
                    )
                } else {
                    format!("parameter `{target_name}` must accept keyword arguments")
                }
            }
            Self::ParameterMustAcceptPositionalArguments { name } => format!(
                "parameter `{name}` is keyword-only but must also accept positional arguments",
            ),
            Self::TupleLengthMismatch {
                source_len,
                target_len,
            } => format!(
                "a tuple of length {source_len} is not {} a tuple of length {}",
                relation.description(),
                target_len.display_minimum(),
            ),
            Self::TupleElementNotCompatible {
                source,
                target,
                element_index,
                element_count,
            } => {
                let which = match (*element_index, *element_count) {
                    (1, _) => "the first tuple element".to_string(),
                    (2, _) => "the second tuple element".to_string(),
                    (n, c) if n == c => "the last tuple element".to_string(),
                    (3, _) => "the third tuple element".to_string(),
                    (n, c) => format!("tuple element {n} of {c}"),
                };
                format!(
                    "{which} is not compatible: `{source}` is not {relation} `{target}`",
                    source = source.display(db, env),
                    relation = relation.description(),
                    target = target.display(db, env)
                )
            }
            Self::TypeNotCompatibleWithProtocol { ty, protocol } => {
                if let Type::ProtocolInstance(_) = ty {
                    format!(
                        "protocol `{}` is not {} protocol `{}`",
                        ty.display(db, env),
                        relation.description(),
                        protocol.display(db, env),
                    )
                } else {
                    format!(
                        "type `{}` is not {} protocol `{}`",
                        ty.display(db, env),
                        relation.description(),
                        protocol.display(db, env),
                    )
                }
            }
            Self::ProtocolMemberNotDefined { member_name, ty } => format!(
                "protocol member `{member_name}` is not defined on type `{}`",
                ty.display(db, env),
            ),
            Self::ProtocolMemberClassVarMismatch { member_name, ty } => format!(
                "protocol member `{member_name}` is an instance variable on type `{}`, but a class variable is required",
                ty.display(db, env),
            ),
            Self::ProtocolSpecialMethodNotDefinedOnMetaType => {
                "special methods must be defined on the meta-type when matching a protocol"
                    .to_string()
            }
            Self::ProtocolMemberIncompatible { member_name } => {
                format!("protocol member `{member_name}` is incompatible")
            }
            Self::ProtocolMemberReadTypeIncompatible { source, target } => format!(
                "read type `{source}` is not {relation} `{target}`",
                source = source.display(db, env),
                relation = relation.description(),
                target = target.display(db, env),
            ),
            Self::ProtocolMemberNotWritable => "the member is not writable".to_string(),
            Self::ProtocolMemberWriteTypeIncompatible { target } => format!(
                "the member does not accept writes of type `{}`",
                target.display(db, env),
            ),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum HelpMessages {
    RequiredFieldCouldBeRemoved,
    TypedDictNotAssignableToDict(TypeRelation),
    ConsiderUsingMappingInsteadOfDict,
    TopCallableExplanation,
    ConsiderAddingADefaultValue {
        parameter_name: Option<Name>,
    },
    OpenTypedDictNotAssignableToMapping {
        typed_dict_name: Option<Name>,
        relation: TypeRelation,
    },
    ExplainOpenTypedDictUnsoundness {
        typed_dict_name: Option<Name>,
    },
}

impl std::fmt::Display for HelpMessages {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelpMessages::RequiredFieldCouldBeRemoved => {
                f.write_str("The required field could be removed through a destructive operation like `del` on the target.")
            }
            HelpMessages::TypedDictNotAssignableToDict(relation) => {
                write!(
                    f,
                    "A TypedDict is not usually {} any `dict[..]` type; \
                    `dict` types allow destructive operations like `clear()`.",
                    relation.description()
                )
            }
            HelpMessages::ConsiderUsingMappingInsteadOfDict => {
                f.write_str("Consider using `Mapping[..]` instead of `dict[..]`.")
            }
            HelpMessages::OpenTypedDictNotAssignableToMapping {typed_dict_name, relation} => {
                let name = typed_dict_name.as_ref().map(|name|format!("`{name}`")).unwrap_or_else(||"this TypedDict".to_string());
                write!(
                    f,
                    "{name} would be {relation} this `Mapping` type \
                    if it were declared with `closed=True`, but TypedDicts are open by default.",
                    relation = relation.description()
                )
            }
            HelpMessages::ExplainOpenTypedDictUnsoundness {typed_dict_name} => {
                let name = typed_dict_name.as_ref().map(|name|format!("`{name}`")).unwrap_or_else(||"this TypedDict".to_string());
                write!(
                    f,
                    "A subclass of {name} could validly add a new field of an arbitrary type, \
                    violating subtyping with the `Mapping` type"
                )
            }
            HelpMessages::TopCallableExplanation => f.write_str(
                "This type includes all possible parameter sets, \
                so it cannot safely be called because there is no valid set of arguments for it",
            ),
            HelpMessages::ConsiderAddingADefaultValue { parameter_name } => match parameter_name {
                Some(name) => write!(f, "Parameter `{name}` must have a default value"),
                None => f.write_str("The parameter must have a default value"),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ErrorContextNode<'db> {
    context: ErrorContext<'db>,
    children: Vec<ErrorContextNode<'db>>,
}

impl Default for ErrorContextNode<'_> {
    fn default() -> Self {
        Self {
            context: ErrorContext::Empty,
            children: Vec::new(),
        }
    }
}

impl<'db> ErrorContextNode<'db> {
    /// Returns `true` if this node has no renderable content.
    fn is_empty(&self) -> bool {
        matches!(self.context, ErrorContext::Empty) && self.children.is_empty()
    }

    #[expect(clippy::too_many_arguments)]
    fn render_tree(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        relation: TypeRelation,
        output_lines: &mut Vec<String>,
        help_messages: &mut FxOrderSet<HelpMessages>,
        prefix: &str,
        continuation: &str,
    ) {
        if let Some(line) = self.context.render(db, env, relation, help_messages) {
            output_lines.push(format!("{prefix}{line}"));
        }

        let num_children = self.children.len();
        for (index, child) in self.children.iter().enumerate() {
            let is_last = index == num_children - 1;
            let (child_prefix, child_continuation) = if is_last {
                (format!("{continuation}└── "), format!("{continuation}    "))
            } else {
                (format!("{continuation}├── "), format!("{continuation}│   "))
            };
            child.render_tree(
                db,
                env,
                relation,
                output_lines,
                help_messages,
                &child_prefix,
                &child_continuation,
            );
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ErrorContextTree<'db> {
    root: Rc<RefCell<ErrorContextNode<'db>>>,
    enabled: Cell<bool>,
    relation: TypeRelation,
}

impl PartialEq for ErrorContextTree<'_> {
    fn eq(&self, other: &Self) -> bool {
        *self.root.borrow() == *other.root.borrow() && self.relation == other.relation
    }
}

impl Eq for ErrorContextTree<'_> {}

impl<'db> ErrorContextTree<'db> {
    /// Create a new, empty error context tree with collection enabled.
    pub(crate) fn new(relation: TypeRelation) -> Self {
        Self {
            root: Rc::default(),
            enabled: Cell::new(true),
            relation,
        }
    }

    pub(crate) fn from_context(context: ErrorContext<'db>, relation: TypeRelation) -> Self {
        Self {
            root: Rc::new(RefCell::new(ErrorContextNode {
                context,
                children: Vec::new(),
            })),
            enabled: Cell::new(true),
            relation,
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.get()
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    /// Returns `true` if the tree has no renderable content.
    pub(crate) fn is_empty(&self) -> bool {
        self.root.borrow().is_empty()
    }

    /// Push a new error context node, making the existing tree a child of the new context.
    pub(crate) fn push(&self, context: ErrorContext<'db>) {
        if !self.is_enabled() {
            return;
        }
        let root = self.root.take();
        let children = if root.is_empty() { vec![] } else { vec![root] };
        *self.root.borrow_mut() = ErrorContextNode { context, children };
    }

    /// Overwrite the error context tree with a new root context and child nodes.
    pub(crate) fn set(
        &self,
        context: ErrorContext<'db>,
        children: impl IntoIterator<Item = ErrorContextTree<'db>>,
    ) {
        if !self.is_enabled() {
            return;
        }
        *self.root.borrow_mut() = ErrorContextNode {
            context,
            children: children
                .into_iter()
                .map(|child_context| child_context.root.take())
                .filter(|child| !child.is_empty())
                .collect(),
        };
    }

    /// Return the full tree, replacing it with an empty tree.
    pub(crate) fn take(&self) -> Self {
        ErrorContextTree {
            root: Rc::new(RefCell::new(std::mem::take(&mut *self.root.borrow_mut()))),
            enabled: Cell::new(self.enabled.get()),
            relation: self.relation,
        }
    }

    /// Render the error context tree as info sub-diagnostics on `diag`.
    pub(in crate::types) fn attach_to(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        diag: &mut LintDiagnosticGuard<'_, '_>,
    ) {
        let mut output_lines = Vec::new();
        let mut help_messages = FxOrderSet::default();
        self.root.borrow().render_tree(
            db,
            env,
            self.relation,
            &mut output_lines,
            &mut help_messages,
            "",
            "",
        );
        for line in output_lines {
            diag.info(line);
        }
        for help_message in help_messages {
            diag.help(help_message.to_string());
        }
    }
}
