//! Diagnostics and display-only fixes for non-exhaustive `match` statements.

use std::borrow::Cow;

use itertools::{Either, Itertools};
use ruff_db::diagnostic::{Annotation, Span};
use ruff_db::parsed::parsed_module;
use ruff_db::source::source_text;
use ruff_diagnostics::{Edit, Fix};
use ruff_python_ast as ast;
use ruff_python_trivia::indentation_at_offset;
use ruff_source_file::{LineRanges, UniversalNewlineIterator, find_newline};
use ruff_text_size::{Ranged, TextSize};
use smallvec::SmallVec;
use ty_module_resolver::file_to_module;
use ty_python_core::place::PlaceExprRef;

use crate::diagnostic::format_enumeration;
use crate::importer::ImportRequest;
use crate::types::class::{ClassLiteral, DynamicEnumLiteral};
use crate::types::cyclic::{ActiveRecursionDetector, TypeIdentity};
use crate::types::display::DisplaySettings;
use crate::types::enums::enum_member_literals;
use crate::types::equality::is_same_enum_domain;
use crate::types::literal::LiteralValueTypeKind;
use crate::types::{EnumLiteralType, KnownClass, Type, diagnostic::NON_EXHAUSTIVE_MATCH};
use crate::{Db, FxIndexMap, FxIndexSet, ProgramEnvironment};

use super::TypeInferenceBuilder;

impl<'db> TypeInferenceBuilder<'db, '_> {
    /// Report values left uncovered by a `match` statement.
    ///
    /// `subject_type` is the type at the start of the match; `remaining` is the type left after
    /// all `case` branches have been considered. The diagnostic identifies individual missing
    /// values when possible and may provide a display-only fix.
    pub(super) fn report_non_exhaustive_match(
        &self,
        match_statement: &ast::StmtMatch,
        subject_type: Type<'db>,
        remaining: Type<'db>,
    ) {
        let db = self.db();
        let env = self.program_environment();

        let Some(builder) = self
            .context
            .report_lint(&NON_EXHAUSTIVE_MATCH, &*match_statement.subject)
        else {
            return;
        };

        let missing = finite_values(db, env, remaining);
        let limit = if db.verbose() { usize::MAX } else { 3 };

        let message = if let Some(missing_values) = missing.as_ref()
            && !missing_values.is_empty()
        {
            let display_settings =
                DisplaySettings::from_possibly_ambiguous_types(db, env, missing_values);

            let displayed: Vec<_> = missing_values
                .iter()
                .take(limit)
                .map(|value| value.display_literal_value_with(db, env, display_settings.clone()))
                .collect();

            let omitted = missing_values.len() - displayed.len();

            let names = if omitted > 0 {
                format!(
                    "{} and {omitted} more",
                    displayed
                        .iter()
                        .map(|value| format!("`{value}`"))
                        .join(", ")
                )
            } else if let [one] = displayed.as_slice() {
                format!("`{one}`")
            } else {
                format_enumeration(&displayed)
            };

            let (noun, verb) = match missing_values.as_slice() {
                [value] if value.is_none(db) => ("", "is"),
                [value] if value.is_enum_literal() => ("enum variant ", "is"),
                [_] => ("value ", "is"),
                _ if missing_values.iter().all(Type::is_enum_literal) => ("enum variants ", "are"),
                _ => ("values ", "are"),
            };

            format!("Match is not exhaustive: {noun}{names} {verb} not covered")
        } else {
            format!(
                "Match is not exhaustive: objects of type `{}` are not covered",
                remaining.display(db, env)
            )
        };

        let mut diagnostic = builder.into_diagnostic(&message);

        let subject_type_display = subject_type.display(db, env);

        diagnostic.set_primary_annotation_message(format_args!(
            "Subject has type `{subject_type_display}`",
        ));

        if subject_type.is_dynamic() || subject_type.is_equivalent_to(db, env, remaining) {
            diagnostic.set_concise_message(format_args!(
                "Match is not exhaustive: subject has type `{subject_type_display}`",
            ));
        } else {
            diagnostic.set_concise_message(message);
        }

        if let Some(missing_values) = missing {
            // If the subject is confined to one enum, its annotations need only member names.
            let is_single_enum_subject = missing_values
                .iter()
                .find_map(|value| value.as_enum_literal())
                .is_some_and(|member| is_same_enum_domain(db, env, subject_type, member));

            let display_settings =
                DisplaySettings::from_possibly_ambiguous_types(db, env, &missing_values);

            // Functional enum members share a definition instead of having individual ones.
            // Group their missing names by enum so the second loop can annotate each shared
            // definition once with all of its displayed missing members. Preserve the order in
            // which enums are encountered so their related locations in editor diagnostics remain
            // stable.
            let mut dynamic_enums: FxIndexMap<DynamicEnumLiteral<'_>, SmallVec<[_; 1]>> =
                FxIndexMap::default();

            for value in missing_values.iter().take(limit) {
                let Some(member) = value.as_enum_literal() else {
                    continue;
                };

                let name = if is_single_enum_subject {
                    Either::Left(member.name(db))
                } else {
                    let display =
                        value.display_literal_value_with(db, env, display_settings.clone());
                    Either::Right(display)
                };

                if let Some(definition) = member.definition(db) {
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    diagnostic.annotate(
                        Annotation::secondary(Span::from(definition.focus_range(db, &module)))
                            .message(format_args!("enum variant `{name}` is not covered")),
                    );
                } else if let ClassLiteral::DynamicEnum(class) = member.enum_class(db) {
                    dynamic_enums.entry(class).or_default().push(name);
                }
            }

            for (class, names) in dynamic_enums {
                let names_range = class.definition(db).and_then(|definition| {
                    let module = parsed_module(db, definition.python_file(db)).load(db);
                    definition
                        .kind(db)
                        .value(&module)
                        .and_then(ast::Expr::as_call_expr)
                        .and_then(|call| call.arguments.find_argument_value("names", 1))
                        .map(Ranged::range)
                });

                let span = Span::from(class.scope(db).file(db))
                    .with_range(names_range.unwrap_or_else(|| class.header_range(db)));

                let (label, names, verb) = if let [name] = names.as_slice() {
                    ("Enum variant", format!("`{name}`"), "is")
                } else {
                    ("Enum variants", format_enumeration(&names), "are")
                };

                diagnostic.annotate(
                    Annotation::secondary(span)
                        .message(format_args!("{label} {names} {verb} not covered")),
                );
            }

            if missing_values.len() > limit {
                diagnostic.info(format_args!(
                    "Use `--verbose` to see all {} uncovered values",
                    missing_values.len()
                ));
            }
        }

        if contains_flag_instance(db, env, remaining) {
            diagnostic.info("`enum.Flag` can have unnamed combinations of members");
            diagnostic
                .info("See https://docs.python.org/3/howto/enum.html#combining-members-of-flag");
        }

        if let Some(fix) = self.non_exhaustive_match_fix(match_statement, remaining) {
            diagnostic.help("Add a `case` branch for the remaining values");
            diagnostic.set_fix(fix);
        }
    }

    /// Suggest a display-only `case` for the remaining values at the end of the match.
    ///
    /// List the remaining values as alternatives when all can be enumerated and there are one to
    /// five of them. If any is an enum member, the match must also have no guards and every enum
    /// class must have an existing, unshadowed runtime reference. Otherwise, use a wildcard. The
    /// `case` body raises `NotImplementedError` as a placeholder.
    fn non_exhaustive_match_fix(
        &self,
        match_statement: &ast::StmtMatch,
        remaining: Type<'db>,
    ) -> Option<Fix> {
        let db = self.db();
        let env = self.program_environment();
        let source = source_text(db, self.file());
        let last_case = match_statement.cases.last()?;
        let case_indent = indentation_at_offset(last_case.start(), &source)?;

        let body_indent = last_case
            .body
            .first()
            .and_then(|statement| indentation_at_offset(statement.start(), &source))
            .map(Cow::Borrowed)
            .unwrap_or_else(|| {
                Cow::Owned(format!(
                    "{case_indent}{}",
                    self.context.importer().indentation()
                ))
            });

        let line_ending = find_newline(&source)
            .map(|(_, ending)| ending)
            .unwrap_or_default()
            .as_str();

        let mut end = source.full_line_end(last_case.end());

        for line in UniversalNewlineIterator::with_offset(&source[usize::from(end)..], end) {
            if line.trim().is_empty() {
                continue;
            }
            if line.starts_with(&*body_indent) && line.trim_start().starts_with('#') {
                end = line.full_end();
            } else {
                break;
            }
        }

        let leading_newline = if source.line_start(end) == end {
            ""
        } else {
            line_ending
        };

        // A guard can change an enum reference before the suggested `case` is reached.
        let has_guard = match_statement
            .cases
            .iter()
            .any(|case| case.guard.is_some());

        let pattern = finite_values(db, env, remaining)
            .filter(|values| !values.is_empty() && values.len() <= 5)
            .and_then(|values| {
                values
                    .into_iter()
                    .map(|value| match value.as_enum_literal() {
                        Some(_) if has_guard => None,
                        Some(member) => {
                            let qualifier = self.enum_qualifier(member, match_statement.start())?;
                            Some(Either::Left(format!("{qualifier}.{}", member.name(db))))
                        }
                        None => Some(Either::Right(value.display_literal_value(db, env))),
                    })
                    .collect::<Option<Vec<_>>>()
                    .map(|patterns| patterns.into_iter().join(" | "))
            })
            .unwrap_or_else(|| "_".to_string());

        let insertion = format!(
            "{leading_newline}{case_indent}case {pattern}:{line_ending}\
            {body_indent}raise NotImplementedError(\"TODO\"){line_ending}"
        );

        Some(Fix::display_only_edit(Edit::insertion(insertion, end)))
    }

    /// Find an in-scope reference to the enum class for a suggested `case`.
    ///
    /// Reuse a local definition or an existing runtime import, including an import alias. Return
    /// `None` if the reference has a visible shadowing or reassignment, or needs a new import.
    fn enum_qualifier(
        &self,
        member: EnumLiteralType<'db>,
        at: TextSize,
    ) -> Option<impl std::fmt::Display + 'db> {
        let db = self.db();
        let class = member.enum_class(db);
        let definition = class.definition(db)?;

        if definition.file(db) == self.file() {
            let definition_scope = definition.file_scope(db);
            let symbol_id = definition.place(db).as_symbol()?;
            let symbol = self.index.place_table(definition_scope).symbol(symbol_id);
            let name = symbol.name();

            if definition.full_range(db, self.module()).end() > at {
                return None;
            }

            let (scope, id, symbol) = self
                .index
                .visible_ancestor_scopes(self.scope().file_scope_id(db))
                .find_map(|(scope, _)| {
                    let places = self.index.place_table(scope);
                    let id = places.symbol_id(name)?;
                    let symbol = places.symbol(id);
                    (symbol.is_bound() || symbol.is_declared()).then_some((scope, id, symbol))
                })?;

            if scope != definition_scope || symbol.is_reassigned() {
                return None;
            }

            let mut bindings = self
                .index
                .use_def_map(scope)
                .end_of_scope_symbol_bindings(id);

            if bindings.next()?.binding.definition() != Some(definition) {
                return None;
            }

            return bindings.next().is_none().then_some(Either::Left(name));
        }

        if !definition.file_scope(db).is_global() {
            return None;
        }

        let module = file_to_module(db, class.program_file(db).resolver_file(db))?;
        let name = definition.name(db)?;

        let action = self.context.importer().import_for_diagnostic(
            ImportRequest::import_from(module.name(db), &name),
            self.scope().file_scope_id(db),
            at,
        )?;

        if action.import().is_some() {
            return None;
        }

        let qualifier = action.into_symbol_text();

        for member in self
            .index
            .visible_ancestor_scopes(self.scope().file_scope_id(db))
            .flat_map(|(scope, _)| self.index.place_table(scope).members())
        {
            if !PlaceExprRef::from(member).is_bound() {
                continue;
            }

            let bound = member.to_string();
            if *qualifier == *bound || qualifier.starts_with(&format!("{bound}.")) {
                return None;
            }
        }

        Some(Either::Right(qualifier))
    }
}

/// Return whether `ty` represents a single value usable in a match pattern.
///
/// This includes `None` and enum literals, but excludes `LiteralString`, which describes many
/// possible strings.
fn is_literal_value<'db>(db: &'db dyn Db, ty: Type<'db>) -> bool {
    if ty.is_none(db) {
        return true;
    }

    let Some(kind) = ty.as_literal_value_kind() else {
        return false;
    };

    match kind {
        LiteralValueTypeKind::Int(_)
        | LiteralValueTypeKind::Bool(_)
        | LiteralValueTypeKind::String(_)
        | LiteralValueTypeKind::Bytes(_)
        | LiteralValueTypeKind::Enum(_) => true,
        LiteralValueTypeKind::LiteralString => false,
    }
}

/// Return whether `ty`, or an element of a union in `ty`, is a non-literal `enum.Flag` subtype.
fn contains_flag_instance<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> bool {
    match ty.expand_top_level_aliases(db, env) {
        Type::Union(union) => union
            .elements(db)
            .iter()
            .any(|element| contains_flag_instance(db, env, *element)),

        ty => {
            !is_literal_value(db, ty)
                && ty.is_subtype_of(db, env, KnownClass::Flag.to_instance(db, env))
        }
    }
}

/// Enumerate the possible literal values of `ty` when its type shape is supported.
///
/// This expands unions, enum instances whose members cover all their possible values, enum
/// complements, and intersections with a finite positive component as well as single literal
/// values. Type aliases, type-variable bounds, constraints, and `NewType` bases are expanded. Values
/// that cannot be proven disjoint from an intersection are retained, even if they might not satisfy
/// all its constraints. A type-variable bound or its constraints can include values that are absent
/// from a particular specialization. Returns `None` when enumeration cannot be completed.
fn finite_values<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    ty: Type<'db>,
) -> Option<Vec<Type<'db>>> {
    /// Enumerate `ty`, returning `None` if its type identity is already being expanded.
    fn visit<'db>(
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        ty: Type<'db>,
        active: &ActiveRecursionDetector<TypeIdentity<'db>>,
    ) -> Option<Vec<Type<'db>>> {
        active.visit(
            &ty.to_type_identity(db),
            || None,
            || match ty {
                Type::Union(union) => {
                    let elements = union.elements(db);
                    // Multiple intersection arms can contain the same value. Keep its first occurrence
                    // so the diagnostic lists each value once in a stable order.
                    let mut values = FxIndexSet::default();
                    for element in elements {
                        values.extend(visit(db, env, *element, active)?);
                    }
                    Some(values.into_iter().collect())
                }

                Type::Intersection(intersection) => {
                    let candidates = intersection
                        .positive(db)
                        .iter()
                        .find_map(|element| visit(db, env, *element, active))?;
                    Some(
                        candidates
                            .into_iter()
                            .filter(|candidate| !candidate.is_disjoint_from(db, env, ty))
                            .collect(),
                    )
                }

                Type::NominalInstance(_) if ty.is_none(db) => Some(vec![ty]),

                Type::NominalInstance(instance) => {
                    Some(enum_member_literals(db, instance.class_literal(db, env), None)?.collect())
                }

                Type::LiteralValue(literal) => match literal.kind() {
                    LiteralValueTypeKind::Int(_)
                    | LiteralValueTypeKind::Bool(_)
                    | LiteralValueTypeKind::String(_)
                    | LiteralValueTypeKind::Bytes(_)
                    | LiteralValueTypeKind::Enum(_) => Some(vec![ty]),
                    LiteralValueTypeKind::LiteralString => None,
                },

                Type::EnumComplement(complement) => {
                    let mut values = Vec::new();
                    for value in complement.remaining_literal_types(db, env) {
                        if !value.is_never() {
                            values.extend(visit(db, env, value, active)?);
                        }
                    }
                    Some(values)
                }

                Type::TypeAlias(alias) => visit(db, env, alias.value_type(db), active),

                Type::Recursive(recursive) => {
                    let unfolded = recursive
                        .unfold(db, &recursive.environment(db))
                        .into_unfolded()?;
                    visit(db, env, unfolded, active)
                }

                Type::TypeVar(typevar) => {
                    let bounds = typevar.typevar(db).bound_or_constraints(db, env)?;
                    visit(db, env, bounds.as_type(db, env), active)
                }

                Type::NewTypeInstance(newtype) => {
                    visit(db, env, newtype.concrete_base_type(db), active)
                }

                Type::Dynamic(_)
                | Type::Divergent(_)
                | Type::RecursiveVar(_)
                | Type::Never
                | Type::FunctionLiteral(_)
                | Type::BoundMethod(_)
                | Type::KnownBoundMethod(_)
                | Type::WrapperDescriptor(_)
                | Type::DataclassDecorator(_)
                | Type::DataclassTransformer(_)
                | Type::Callable(_)
                | Type::ModuleLiteral(_)
                | Type::ClassLiteral(_)
                | Type::GenericAlias(_)
                | Type::SubclassOf(_)
                | Type::ProtocolInstance(_)
                | Type::SpecialForm(_)
                | Type::KnownInstance(_)
                | Type::PropertyInstance(_)
                | Type::SlotDescriptor(_)
                | Type::AlwaysTruthy
                | Type::AlwaysFalsy
                | Type::BoundSuper(_)
                | Type::TypeIs(_)
                | Type::TypeGuard(_)
                | Type::TypeForm(_)
                | Type::TypedDict(_) => None,
            },
        )
    }

    visit(db, env, ty, &ActiveRecursionDetector::default())
}
