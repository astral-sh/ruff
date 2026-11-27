//! Capture information retained for regular expressions compiled from literal patterns.

use ty_module_resolver::{KnownModule, file_to_module};

use crate::types::call::{Argument, Binding, CallArguments};
use crate::types::function::{FunctionType, KnownFunction};
use crate::types::regex::{RegexGroup, RegexGroups};
use crate::types::{KnownClass, KnownInstanceType, LiteralValueTypeKind, Type, UnionType};
use crate::{Db, ProgramEnvironment};

/// A pattern, or a match produced by that pattern, whose captures are statically known.
/// The payload describes a set of runtime objects, rather than a single match object.
#[salsa::interned(debug, heap_size=ruff_memory_usage::heap_size)]
pub struct RegexInstance<'db> {
    #[returns(ref)]
    groups: RegexGroups,
    #[returns(copy)]
    is_bytes: bool,
    #[returns(copy)]
    kind: RegexKind,
}

impl get_size2::GetSize for RegexInstance<'_> {}

#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq, get_size2::GetSize, salsa::SalsaValue)]
pub enum RegexKind {
    Pattern,
    Match,
}

impl<'db> RegexInstance<'db> {
    pub(crate) fn class(self, db: &'db dyn Db) -> KnownClass {
        match self.kind(db) {
            RegexKind::Pattern => KnownClass::RePattern,
            RegexKind::Match => KnownClass::ReMatch,
        }
    }

    fn string_type(self, db: &'db dyn Db, env: &ProgramEnvironment<'db>) -> Type<'db> {
        if self.is_bytes(db) {
            KnownClass::Bytes
        } else {
            KnownClass::Str
        }
        .to_instance(db, env)
    }

    pub(crate) fn instance_fallback(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
    ) -> Type<'db> {
        self.class(db)
            .to_specialized_instance(db, env, &[self.string_type(db, env)])
    }

    fn with_kind(self, db: &'db dyn Db, kind: RegexKind) -> Self {
        Self::new(db, self.groups(db), self.is_bytes(db), kind)
    }

    fn into_type(self) -> Type<'db> {
        Type::KnownInstance(KnownInstanceType::Regex(self))
    }

    pub(crate) fn member(self, db: &'db dyn Db, name: &str) -> Option<Type<'db>> {
        match (self.kind(db), name) {
            (RegexKind::Match, "re") => Some(self.with_kind(db, RegexKind::Pattern).into_type()),
            _ => None,
        }
    }

    fn match_result(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        finditer: bool,
    ) -> Type<'db> {
        let matched = self.with_kind(db, RegexKind::Match).into_type();
        if finditer {
            KnownClass::Iterator.to_specialized_instance(db, env, &[matched])
        } else {
            UnionType::from_two_elements(db, env, matched, Type::none(db, env))
        }
    }

    fn capture_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        group: &RegexGroup,
        default: Type<'db>,
    ) -> Type<'db> {
        let string = self.string_type(db, env);
        if group.is_required {
            string
        } else {
            UnionType::from_two_elements(db, env, string, default)
        }
    }

    fn group_type(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        key: Type<'db>,
    ) -> Type<'db> {
        if let Type::Union(union) = key {
            return union.map(db, env, |key| self.group_type(db, env, *key));
        }
        let groups = self.groups(db);
        let group = if let Some(index) = key.as_int_like_literal() {
            usize::try_from(index)
                .ok()
                .and_then(|index| groups.group(index))
        } else if let Some(name) = key.as_string_literal() {
            groups.named_group(name.value(db))
        } else {
            None
        };
        if let Some(group) = group {
            return self.capture_type(db, env, group, Type::none(db, env));
        }
        // Unknown indices can select any capture (or raise IndexError). Group zero always
        // participates, so the result is a string when all other captures are required too.
        if groups.groups().iter().all(|group| group.is_required) {
            self.string_type(db, env)
        } else {
            UnionType::from_two_elements(db, env, self.string_type(db, env), Type::none(db, env))
        }
    }

    pub(crate) fn infer_method_call(
        self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        name: &str,
        binding: &Binding<'db>,
        arguments: &CallArguments<'_, 'db>,
    ) -> Option<Type<'db>> {
        match (self.kind(db), name) {
            (RegexKind::Pattern, "search" | "match" | "fullmatch" | "finditer") => {
                Some(self.match_result(db, env, name == "finditer"))
            }
            (_, "__copy__" | "__deepcopy__") => Some(self.into_type()),
            (RegexKind::Match, "group" | "__getitem__") => {
                // Preserve each positional argument: binding the variadic `groups`
                // parameter merges its arguments and loses their order and tuple length.
                let mut elements = Vec::new();
                for (argument, types) in arguments.iter().skip(1) {
                    let ty = types.get_default()?;
                    match argument {
                        Argument::Positional => elements.push(self.group_type(db, env, ty)),
                        Argument::Variadic => {
                            let tuple = ty.as_nominal_instance()?.tuple_spec(db, env)?;
                            elements.extend(
                                tuple
                                    .as_fixed_length()?
                                    .iter_all_elements()
                                    .map(|ty| self.group_type(db, env, ty)),
                            );
                        }
                        _ => return None,
                    }
                }
                Some(match elements.as_slice() {
                    [] => self.string_type(db, env),
                    [element] => *element,
                    _ => Type::heterogeneous_tuple(db, env, elements),
                })
            }
            (RegexKind::Match, "groups" | "groupdict") => {
                let default = match binding.parameter_types() {
                    [_] | [_, None] => Type::none(db, env),
                    [_, Some(default)] => *default,
                    _ => return None,
                };
                let groups = self.groups(db).groups();
                if name == "groups" {
                    Some(Type::heterogeneous_tuple(
                        db,
                        env,
                        groups
                            .iter()
                            .map(|group| self.capture_type(db, env, group, default)),
                    ))
                } else {
                    if !groups.iter().any(|group| group.name.is_some()) {
                        return None;
                    }
                    let value = UnionType::from_elements(
                        db,
                        env,
                        groups
                            .iter()
                            .filter(|group| group.name.is_some())
                            .map(|group| self.capture_type(db, env, group, default)),
                    );
                    Some(KnownClass::Dict.to_specialized_instance(
                        db,
                        env,
                        &[KnownClass::Str.to_instance(db, env), value],
                    ))
                }
            }
            _ => None,
        }
    }
}

pub(crate) fn is_re_function(db: &dyn Db, function: FunctionType<'_>) -> bool {
    file_to_module(
        db,
        function.definition(db).program_file(db).resolver_file(db),
    )
    .is_some_and(|module| module.known(db) == Some(KnownModule::Re))
}

fn flags_value(db: &dyn Db, flags: Option<Type<'_>>) -> Option<i64> {
    let Some(flags) = flags else { return Some(0) };
    if let Some(value) = flags.as_int_like_literal() {
        return Some(value);
    }
    if let Some(LiteralValueTypeKind::Enum(literal)) = flags.as_literal_value_kind() {
        return literal
            .enum_class_literal(db)
            .value_type(db, literal.name(db))?
            .as_int_like_literal();
    }
    None
}

pub(crate) fn infer_function_call<'db>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    function: KnownFunction,
    parameters: &[Option<Type<'db>>],
) -> Option<Type<'db>> {
    let (pattern, flags) = match parameters {
        [Some(pattern), flags] if function == KnownFunction::ReCompile => (*pattern, *flags),
        [Some(pattern), Some(_), flags] => (*pattern, *flags),
        _ => return None,
    };
    let flags = flags_value(db, flags)?;
    let regex = match pattern {
        Type::KnownInstance(KnownInstanceType::Regex(regex))
            if regex.kind(db) == RegexKind::Pattern && flags == 0 =>
        {
            regex
        }
        _ => {
            let (groups, is_bytes) = match pattern.as_literal_value_kind()? {
                LiteralValueTypeKind::String(pattern) => (
                    RegexGroups::parse(pattern.value(db), flags, false, env.python_version(db))?,
                    false,
                ),
                LiteralValueTypeKind::Bytes(pattern) => {
                    // Latin-1 preserves regex syntax and maps each byte to one character.
                    let pattern: String = pattern
                        .value(db)
                        .iter()
                        .map(|byte| char::from(*byte))
                        .collect();
                    (
                        RegexGroups::parse(&pattern, flags, true, env.python_version(db))?,
                        true,
                    )
                }
                _ => return None,
            };
            RegexInstance::new(db, groups, is_bytes, RegexKind::Pattern)
        }
    };
    Some(if function == KnownFunction::ReCompile {
        regex.into_type()
    } else {
        regex.match_result(db, env, function == KnownFunction::ReFinditer)
    })
}
