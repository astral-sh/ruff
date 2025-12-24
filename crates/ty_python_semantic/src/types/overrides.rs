//! Checks relating to invalid method overrides in subclasses,
//! including (but not limited to) violations of the [Liskov Substitution Principle].
//!
//! [Liskov Substitution Principle]: https://en.wikipedia.org/wiki/Liskov_substitution_principle

use bitflags::bitflags;
use ruff_db::diagnostic::Annotation;
use rustc_hash::FxHashSet;

use crate::{
    Db,
    lint::LintId,
    place::Place,
    semantic_index::{
        definition::DefinitionKind, place::ScopedPlaceId, place_table, scope::ScopeId,
        symbol::ScopedSymbolId, use_def_map,
    },
    types::{
        ClassBase, ClassLiteral, ClassType, KnownClass, Type,
        class::CodeGeneratorKind,
        context::InferContext,
        diagnostic::{
            INVALID_EXPLICIT_OVERRIDE, INVALID_METHOD_OVERRIDE, INVALID_NAMED_TUPLE,
            OVERRIDE_OF_FINAL_METHOD, report_invalid_method_override,
            report_overridden_final_method,
        },
        function::{FunctionDecorators, FunctionType, KnownFunction},
        list_members::{Member, MemberWithDefinition, all_end_of_scope_members},
    },
};

/// Prohibited `NamedTuple` attributes that cannot be overwritten.
/// See <https://github.com/python/cpython/blob/main/Lib/typing.py> for the list.
const PROHIBITED_NAMEDTUPLE_ATTRS: &[&str] = &[
    "__new__",
    "__init__",
    "__slots__",
    "__getnewargs__",
    "_fields",
    "_field_defaults",
    "_field_types",
    "_make",
    "_replace",
    "_asdict",
    "_source",
];

pub(super) fn check_class<'db>(context: &InferContext<'db, '_>, class: ClassLiteral<'db>) {
    let db = context.db();
    let configuration = OverrideRulesConfig::from(context);
    if configuration.no_rules_enabled() {
        return;
    }

    let class_specialized = class.identity_specialization(db);
    let scope = class.body_scope(db);
    let own_class_members: FxHashSet<_> = all_end_of_scope_members(db, scope).collect();

    for member in own_class_members {
        check_class_declaration(context, configuration, class_specialized, scope, &member);
    }
}

fn check_class_declaration<'db>(
    context: &InferContext<'db, '_>,
    configuration: OverrideRulesConfig,
    class: ClassType<'db>,
    class_scope: ScopeId<'db>,
    member: &MemberWithDefinition<'db>,
) {
    /// Salsa-tracked query to check whether any of the definitions of a symbol
    /// in a superclass scope are function definitions.
    ///
    /// We need to know this for compatibility with pyright and mypy, neither of which emit an error
    /// on `C.f` here:
    ///
    /// ```python
    /// from typing import final
    ///
    /// class A:
    ///     @final
    ///     def f(self) -> None: ...
    ///
    /// class B:
    ///     f = A.f
    ///
    /// class C(B):
    ///     def f(self) -> None: ...  # no error here
    /// ```
    ///
    /// This is a Salsa-tracked query because it has to look at the AST node for the definition,
    /// which might be in a different Python module. If this weren't a tracked query, we could
    /// introduce cross-module dependencies and over-invalidation.
    #[salsa::tracked(heap_size=ruff_memory_usage::heap_size)]
    fn is_function_definition<'db>(
        db: &'db dyn Db,
        scope: ScopeId<'db>,
        symbol: ScopedSymbolId,
    ) -> bool {
        use_def_map(db, scope)
            .end_of_scope_symbol_bindings(symbol)
            .filter_map(|binding| binding.binding.definition())
            .any(|definition| definition.kind(db).is_function_def())
    }

    let db = context.db();

    let MemberWithDefinition {
        member,
        first_reachable_definition,
    } = member;

    let Place::Defined(type_on_subclass_instance, _, _, _) =
        Type::instance(db, class).member(db, &member.name).place
    else {
        return;
    };

    let (literal, specialization) = class.class_literal(db);
    let class_kind = CodeGeneratorKind::from_class(db, literal, specialization);

    // Check for prohibited `NamedTuple` attribute overrides.
    //
    // `NamedTuple` classes have certain synthesized attributes (like `_asdict`, `_make`, etc.)
    // that cannot be overwritten. Attempting to assign to these attributes (without type
    // annotations) or define methods with these names will raise an `AttributeError` at runtime.
    if class_kind == Some(CodeGeneratorKind::NamedTuple)
        && configuration.check_prohibited_named_tuple_attrs()
        && PROHIBITED_NAMEDTUPLE_ATTRS.contains(&member.name.as_str())
        && let Some(symbol_id) = place_table(db, class_scope).symbol_id(&member.name)
        && let Some(bad_definition) = use_def_map(db, class_scope)
            .reachable_bindings(ScopedPlaceId::Symbol(symbol_id))
            .filter_map(|binding| binding.binding.definition())
            .find(|def| !matches!(def.kind(db), DefinitionKind::AnnotatedAssignment(_)))
        && let Some(builder) = context.report_lint(
            &INVALID_NAMED_TUPLE,
            bad_definition.focus_range(db, context.module()),
        )
    {
        let mut diagnostic = builder.into_diagnostic(format_args!(
            "Cannot overwrite NamedTuple attribute `{}`",
            &member.name
        ));
        diagnostic.info("This will cause the class creation to fail at runtime");
    }

    let mut subclass_overrides_superclass_declaration = false;
    let mut has_dynamic_superclass = false;
    let mut has_typeddict_in_mro = false;
    let mut liskov_diagnostic_emitted = false;
    let mut overridden_final_method = None;

    for class_base in class.iter_mro(db).skip(1) {
        let superclass = match class_base {
            ClassBase::Protocol | ClassBase::Generic => continue,
            ClassBase::Dynamic(_) => {
                has_dynamic_superclass = true;
                continue;
            }
            ClassBase::TypedDict => {
                has_typeddict_in_mro = true;
                continue;
            }
            ClassBase::Class(class) => class,
        };

        let (superclass_literal, superclass_specialization) = superclass.class_literal(db);
        let superclass_scope = superclass_literal.body_scope(db);
        let superclass_symbol_table = place_table(db, superclass_scope);
        let superclass_symbol_id = superclass_symbol_table.symbol_id(&member.name);

        let mut method_kind = MethodKind::default();

        // If the member is not defined on the class itself, skip it
        if let Some(id) = superclass_symbol_id {
            let superclass_symbol = superclass_symbol_table.symbol(id);
            if !(superclass_symbol.is_bound() || superclass_symbol.is_declared()) {
                continue;
            }
        } else {
            if superclass_literal
                .own_synthesized_member(db, superclass_specialization, None, &member.name)
                .is_none()
            {
                continue;
            }
            method_kind =
                CodeGeneratorKind::from_class(db, superclass_literal, superclass_specialization)
                    .map(MethodKind::Synthesized)
                    .unwrap_or_default();
        }

        let Place::Defined(superclass_type, _, _, _) = Type::instance(db, superclass)
            .member(db, &member.name)
            .place
        else {
            // If not defined on any superclass, no point in continuing to walk up the MRO
            break;
        };

        subclass_overrides_superclass_declaration = true;

        if configuration.check_final_method_overridden() {
            overridden_final_method = overridden_final_method.or_else(|| {
                let superclass_symbol_id = superclass_symbol_id?;

                // TODO: `@final` should be more like a type qualifier:
                // we should also recognise `@final`-decorated methods that don't end up
                // as being function- or property-types (because they're wrapped by other
                // decorators that transform the type into something else).
                let underlying_functions = extract_underlying_functions(
                    db,
                    superclass
                        .own_class_member(db, None, &member.name)
                        .ignore_possibly_undefined()?,
                )?;

                if underlying_functions
                    .iter()
                    .any(|function| function.has_known_decorator(db, FunctionDecorators::FINAL))
                    && is_function_definition(db, superclass_scope, superclass_symbol_id)
                {
                    Some((superclass, underlying_functions))
                } else {
                    None
                }
            });
        }

        // **********************************************************
        // Everything below this point in the loop
        // is about Liskov Substitution Principle checks
        // **********************************************************

        // Only one Liskov diagnostic should be emitted per each invalid override,
        // even if it overrides multiple superclasses incorrectly!
        if liskov_diagnostic_emitted {
            continue;
        }

        if !configuration.check_method_liskov_violations() {
            continue;
        }

        // TODO: Check Liskov on non-methods too
        let Type::FunctionLiteral(subclass_function) = member.ty else {
            continue;
        };

        // Constructor methods are not checked for Liskov compliance
        if matches!(
            &*member.name,
            "__init__" | "__new__" | "__post_init__" | "__init_subclass__"
        ) {
            continue;
        }

        // Synthesized `__replace__` methods on dataclasses are not checked
        if &member.name == "__replace__"
            && matches!(class_kind, Some(CodeGeneratorKind::DataclassLike(_)))
        {
            continue;
        }

        let Some(superclass_type_as_callable) = superclass_type.try_upcast_to_callable(db) else {
            continue;
        };

        if type_on_subclass_instance.is_assignable_to(db, superclass_type_as_callable.into_type(db))
        {
            continue;
        }

        report_invalid_method_override(
            context,
            &member.name,
            class,
            *first_reachable_definition,
            subclass_function,
            superclass,
            superclass_type,
            method_kind,
        );

        liskov_diagnostic_emitted = true;
    }

    if !subclass_overrides_superclass_declaration && !has_dynamic_superclass {
        if has_typeddict_in_mro {
            if !KnownClass::TypedDictFallback
                .to_instance(db)
                .member(db, &member.name)
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = true;
            }
        } else if class_kind == Some(CodeGeneratorKind::NamedTuple) {
            if !KnownClass::NamedTupleFallback
                .to_instance(db)
                .member(db, &member.name)
                .place
                .is_undefined()
            {
                subclass_overrides_superclass_declaration = true;
            }
        }
    }

    if !subclass_overrides_superclass_declaration
        && !has_dynamic_superclass
        // accessing `.kind()` here is fine as `definition`
        // will always be a definition in the file currently being checked
        && first_reachable_definition.kind(db).is_function_def()
    {
        check_explicit_overrides(context, member, class);
    }

    if let Some((superclass, superclass_method)) = overridden_final_method {
        report_overridden_final_method(
            context,
            &member.name,
            *first_reachable_definition,
            member.ty,
            superclass,
            class,
            &superclass_method,
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum MethodKind<'db> {
    Synthesized(CodeGeneratorKind<'db>),
    #[default]
    NotSynthesized,
}

bitflags! {
    /// Bitflags representing which override-related rules have been enabled.
    #[derive(Default, Debug, Copy, Clone)]
    struct OverrideRulesConfig: u8 {
        const LISKOV_METHODS = 1 << 0;
        const EXPLICIT_OVERRIDE = 1 << 1;
        const FINAL_METHOD_OVERRIDDEN = 1 << 2;
        const PROHIBITED_NAMED_TUPLE_ATTR = 1 << 3;
    }
}

impl From<&InferContext<'_, '_>> for OverrideRulesConfig {
    fn from(value: &InferContext<'_, '_>) -> Self {
        let db = value.db();
        let rule_selection = db.rule_selection(value.file());

        let mut config = OverrideRulesConfig::empty();

        if rule_selection.is_enabled(LintId::of(&INVALID_METHOD_OVERRIDE)) {
            config |= OverrideRulesConfig::LISKOV_METHODS;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_EXPLICIT_OVERRIDE)) {
            config |= OverrideRulesConfig::EXPLICIT_OVERRIDE;
        }
        if rule_selection.is_enabled(LintId::of(&OVERRIDE_OF_FINAL_METHOD)) {
            config |= OverrideRulesConfig::FINAL_METHOD_OVERRIDDEN;
        }
        if rule_selection.is_enabled(LintId::of(&INVALID_NAMED_TUPLE)) {
            config |= OverrideRulesConfig::PROHIBITED_NAMED_TUPLE_ATTR;
        }

        config
    }
}

impl OverrideRulesConfig {
    const fn no_rules_enabled(self) -> bool {
        self.is_empty()
    }

    const fn check_method_liskov_violations(self) -> bool {
        self.contains(OverrideRulesConfig::LISKOV_METHODS)
    }

    const fn check_final_method_overridden(self) -> bool {
        self.contains(OverrideRulesConfig::FINAL_METHOD_OVERRIDDEN)
    }

    const fn check_prohibited_named_tuple_attrs(self) -> bool {
        self.contains(OverrideRulesConfig::PROHIBITED_NAMED_TUPLE_ATTR)
    }
}

fn check_explicit_overrides<'db>(
    context: &InferContext<'db, '_>,
    member: &Member<'db>,
    class: ClassType<'db>,
) {
    let db = context.db();
    let underlying_functions = extract_underlying_functions(db, member.ty);
    let Some(functions) = underlying_functions else {
        return;
    };
    let Some(decorated_function) = functions
        .iter()
        .find(|function| function.has_known_decorator(db, FunctionDecorators::OVERRIDE))
    else {
        return;
    };
    let function_literal = if context.in_stub() {
        decorated_function.first_overload_or_implementation(db)
    } else {
        decorated_function.literal(db).last_definition(db)
    };

    let Some(builder) = context.report_lint(
        &INVALID_EXPLICIT_OVERRIDE,
        function_literal.focus_range(db, context.module()),
    ) else {
        return;
    };
    let mut diagnostic = builder.into_diagnostic(format_args!(
        "Method `{}` is decorated with `@override` but does not override anything",
        member.name
    ));
    if let Some(decorator_span) =
        function_literal.find_known_decorator_span(db, KnownFunction::Override)
    {
        diagnostic.annotate(Annotation::secondary(decorator_span));
    }
    diagnostic.info(format_args!(
        "No `{member}` definitions were found on any superclasses of `{class}`",
        member = &member.name,
        class = class.name(db)
    ));
}

fn extract_underlying_functions<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<smallvec::SmallVec<[FunctionType<'db>; 1]>> {
    match ty {
        Type::FunctionLiteral(function) => Some(smallvec::smallvec_inline![function]),
        Type::BoundMethod(method) => Some(smallvec::smallvec_inline![method.function(db)]),
        Type::PropertyInstance(property) => extract_underlying_functions(db, property.getter(db)?),
        Type::Union(union) => {
            let mut functions = smallvec::smallvec![];
            for member in union.elements(db) {
                if let Some(mut member_functions) = extract_underlying_functions(db, *member) {
                    functions.append(&mut member_functions);
                }
            }
            if functions.is_empty() {
                None
            } else {
                Some(functions)
            }
        }
        _ => None,
    }
}
