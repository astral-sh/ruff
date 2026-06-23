use ruff_db::parsed::parsed_module;
use ruff_python_ast::{self as ast, PythonVersion, name::Name};
use rustc_hash::FxHashMap;
use ty_module_resolver::{KnownModule, file_to_module};

use crate::Db;
use crate::types::{
    ClassLiteral, EnumLiteralType, KnownClass, LiteralValueTypeKind, Program, Type, UnionType,
    definition_expression_type,
};
use ty_python_core::definition::Definition;

use super::{
    EnumClassLiteral, EnumMetadata, custom_enum_method, enum_uses_standard_metaclass_call,
};

/// The policy used when a `Flag` constructor receives bits that are not declared by the class.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub enum FlagBoundary {
    Strict,
    Conform,
    Eject,
    Keep,
    Unknown,
}

impl FlagBoundary {
    pub(crate) const fn default_for_base(base: KnownClass) -> Self {
        match base {
            KnownClass::IntFlag => Self::Keep,
            KnownClass::Flag => Self::Strict,
            _ => Self::Unknown,
        }
    }

    /// Resolve a value of the standard-library `FlagBoundary` enum.
    pub(crate) fn from_type(db: &dyn Db, ty: Type<'_>) -> Option<Self> {
        let LiteralValueTypeKind::Enum(boundary) =
            ty.resolve_type_alias(db).as_literal_value_kind()?
        else {
            return None;
        };
        let ClassLiteral::Static(boundary_class) = boundary.enum_class(db) else {
            return None;
        };
        if boundary_class.name(db) != "FlagBoundary"
            || file_to_module(db, boundary_class.file(db)).and_then(|module| module.known(db))
                != Some(KnownModule::Enum)
        {
            return None;
        }
        match boundary.name(db).as_str() {
            "STRICT" => Some(Self::Strict),
            "CONFORM" => Some(Self::Conform),
            "EJECT" => Some(Self::Eject),
            "KEEP" => Some(Self::Keep),
            _ => None,
        }
    }
}

/// Cached integer semantics for a `Flag` class.
///
/// A profile stores only masks and direct lookup tables. It never enumerates the combinations of
/// declared flags, so its size and construction cost are linear in the number of declared names.
#[derive(Debug, PartialEq, Eq, salsa::Update, get_size2::GetSize)]
pub(crate) struct FlagMetadata<'db> {
    boundary: FlagBoundary,
    member_type: Option<Type<'db>>,
    preserves_negative_values: bool,
    canonical_members_are_known: bool,
    member_values: FxHashMap<Name, i64>,
    named_values: FxHashMap<i64, Name>,
    canonical_members: Box<[(Name, i64)]>,
    flag_mask: Option<i64>,
    singles_mask: Option<i64>,
}

struct FlagMemberType<'db> {
    ty: Option<Type<'db>>,
    values_are_known: bool,
}

fn flag_member_type<'db>(db: &'db dyn Db, class: ClassLiteral<'db>) -> FlagMemberType<'db> {
    match class {
        ClassLiteral::Static(class) => {
            for base in class
                .iter_mro(db, None)
                .skip(1)
                .filter_map(crate::types::ClassBase::into_class)
                .filter_map(|base| base.class_literal(db).as_static())
            {
                match base.known(db) {
                    Some(KnownClass::Flag) => {
                        return FlagMemberType {
                            ty: None,
                            values_are_known: true,
                        };
                    }
                    Some(KnownClass::IntFlag | KnownClass::Int) => {
                        return FlagMemberType {
                            ty: Some(KnownClass::Int.to_instance(db)),
                            values_are_known: true,
                        };
                    }
                    _ if Type::ClassLiteral(ClassLiteral::Static(base))
                        .is_subtype_of(db, KnownClass::Flag.to_subclass_of(db)) => {}
                    _ => {
                        let base = ClassLiteral::Static(base);
                        return FlagMemberType {
                            ty: Type::ClassLiteral(base)
                                .is_subtype_of(db, KnownClass::Int.to_subclass_of(db))
                                .then(|| base.to_non_generic_instance(db)),
                            values_are_known: false,
                        };
                    }
                }
            }
            FlagMemberType {
                ty: None,
                values_are_known: false,
            }
        }
        ClassLiteral::DynamicEnum(enum_lit) => match enum_lit.mixin_type(db) {
            Some(mixin)
                if mixin
                    .to_class_type(db)
                    .is_some_and(|mixin| mixin.known(db) == Some(KnownClass::Int)) =>
            {
                FlagMemberType {
                    ty: Some(KnownClass::Int.to_instance(db)),
                    values_are_known: true,
                }
            }
            Some(_) => FlagMemberType {
                ty: None,
                values_are_known: false,
            },
            None => FlagMemberType {
                ty: (enum_lit.base_class(db) == KnownClass::IntFlag)
                    .then(|| KnownClass::Int.to_instance(db)),
                values_are_known: true,
            },
        },
        ClassLiteral::Dynamic(_)
        | ClassLiteral::DynamicNamedTuple(_)
        | ClassLiteral::DynamicTypedDict(_) => FlagMemberType {
            ty: None,
            values_are_known: false,
        },
    }
}

impl<'db> FlagMetadata<'db> {
    pub(super) fn from_enum_metadata(
        db: &'db dyn Db,
        class: ClassLiteral<'db>,
        metadata: &EnumMetadata<'db>,
        mut boundary: FlagBoundary,
    ) -> Self {
        let mut member_values = FxHashMap::default();
        let mut named_values = FxHashMap::default();
        let mut canonical_members = Vec::new();
        let mut flag_mask = 0_i64;
        let mut singles_mask = 0_i64;
        let mut all_values_are_known = true;
        let mut masks_are_known = true;
        let member_type = flag_member_type(db, class);
        let preserves_negative_values = Program::get(db).python_version(db) < PythonVersion::PY311
            && Type::ClassLiteral(class).is_subtype_of(db, KnownClass::IntFlag.to_subclass_of(db));

        if member_type.values_are_known {
            for name in metadata.members.keys() {
                if metadata.member_value_may_be_transformed(name) {
                    all_values_are_known = false;
                    masks_are_known = false;
                    continue;
                }
                let value_ty = metadata.value_type(db, name);
                let value = value_ty.and_then(Type::as_int_like_literal).or_else(|| {
                    if value_ty?.is_assignable_to(db, KnownClass::Int.to_instance(db)) {
                        metadata.members.get(name)?.as_int_like_literal()
                    } else {
                        None
                    }
                });
                let Some(value) = value else {
                    all_values_are_known = false;
                    masks_are_known = false;
                    continue;
                };

                member_values.insert(name.clone(), value);
                named_values.entry(value).or_insert_with(|| name.clone());
                flag_mask |= value;
                if is_positive_power_of_two(value) {
                    singles_mask |= value;
                    canonical_members.push((name.clone(), value));
                }
            }
        }

        if !member_type.values_are_known {
            all_values_are_known = false;
            masks_are_known = false;
        }

        if metadata.value_construction.metaclass_may_transform_values {
            all_values_are_known = false;
            masks_are_known = false;
            boundary = FlagBoundary::Unknown;
        }

        member_values.shrink_to_fit();
        named_values.shrink_to_fit();

        Self {
            boundary,
            member_type: member_type.ty,
            preserves_negative_values,
            canonical_members_are_known: all_values_are_known,
            member_values,
            named_values,
            canonical_members: canonical_members.into_boxed_slice(),
            flag_mask: masks_are_known.then_some(flag_mask),
            singles_mask: masks_are_known.then_some(singles_mask),
        }
    }

    pub(crate) const fn boundary(&self) -> FlagBoundary {
        self.boundary
    }

    fn accepts_operand(&self, db: &dyn Db, operand: Type<'db>) -> bool {
        self.member_type
            .is_some_and(|member_type| operand.is_assignable_to(db, member_type))
    }

    pub(crate) const fn canonical_members_are_known(&self) -> bool {
        self.canonical_members_are_known
    }

    pub(crate) fn canonical_members(&self) -> &[(Name, i64)] {
        &self.canonical_members
    }

    fn member_value(&self, name: &Name) -> Option<i64> {
        self.member_values.get(name).copied()
    }

    fn named_member(&self, value: i64) -> Option<&Name> {
        self.named_values.get(&value)
    }

    fn all_bits(&self) -> Option<i64> {
        let flag_mask = self.flag_mask?;
        if flag_mask == 0 {
            return Some(0);
        }
        let bit_length = i64::BITS - flag_mask.unsigned_abs().leading_zeros();
        if bit_length == 63 {
            Some(i64::MAX)
        } else {
            1_i64.checked_shl(bit_length).map(|value| value - 1)
        }
    }

    fn value_is_out_of_range(&self, value: i64) -> Option<bool> {
        let flag_mask = self.flag_mask?;
        let all_bits = self.all_bits()?;
        Some(value < !all_bits || value > all_bits || value & (all_bits ^ flag_mask) != 0)
    }

    fn normalize_negative(&self, value: i64) -> Option<i64> {
        if value >= 0 {
            Some(value)
        } else {
            i64::try_from(i128::from(self.all_bits()?) + 1 + i128::from(value)).ok()
        }
    }

    fn normalize_kept_negative(&self, value: i64) -> Option<i64> {
        let all_bits = i128::from(self.all_bits()?);
        let bit_length = i64::BITS - value.unsigned_abs().leading_zeros();
        let modulus = 1_i128.checked_shl(bit_length)?;
        i64::try_from((all_bits + 1).max(modulus) + i128::from(value)).ok()
    }

    fn construct(&self, value: i64) -> FlagConstruction {
        // Enum value lookup returns an existing named member before `Flag._missing_` applies the
        // boundary policy. This matters for explicitly declared negative values that would not
        // otherwise satisfy the class's effective mask.
        if self.named_member(value).is_some() {
            return FlagConstruction::Flag(value);
        }
        if self.preserves_negative_values && value < 0 {
            return FlagConstruction::Flag(value);
        }

        let Some(out_of_range) = self.value_is_out_of_range(value) else {
            return FlagConstruction::Unknown;
        };

        let value = match (self.boundary, out_of_range) {
            (FlagBoundary::Unknown, true) => return FlagConstruction::Unknown,
            (FlagBoundary::Eject, true) => return FlagConstruction::Ejected(value),
            (FlagBoundary::Conform, true) => value & self.flag_mask.unwrap_or_default(),
            (FlagBoundary::Keep, true) if value < 0 => {
                return self
                    .normalize_kept_negative(value)
                    .map_or(FlagConstruction::Unknown, FlagConstruction::Flag);
            }
            // A strict construction raises at runtime. The caller retains the nominal result type
            // because ty does not generally use constructor exceptions to infer `Never`.
            (FlagBoundary::Strict, true) => return FlagConstruction::Raises,
            (FlagBoundary::Keep | FlagBoundary::Strict | FlagBoundary::Conform, _)
            | (FlagBoundary::Unknown | FlagBoundary::Eject, false) => value,
        };

        self.normalize_negative(value)
            .map_or(FlagConstruction::Unknown, FlagConstruction::Flag)
    }
}

#[derive(Clone, Copy)]
enum FlagConstruction {
    Flag(i64),
    Ejected(i64),
    Raises,
    Unknown,
}

/// Values used by the standard `Flag._generate_next_value_` implementation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FlagAutoValueState {
    maximum: Option<i64>,
    last: Option<i64>,
    all_values_are_known: bool,
    has_values: bool,
}

impl FlagAutoValueState {
    pub(crate) const fn new() -> Self {
        Self {
            maximum: None,
            last: None,
            all_values_are_known: true,
            has_values: false,
        }
    }

    pub(crate) fn observe(&mut self, value: Type<'_>) {
        self.has_values = true;
        if let Some(value) = value.as_int_like_literal() {
            self.maximum = Some(self.maximum.map_or(value, |maximum| maximum.max(value)));
            self.last = Some(value);
        } else {
            self.last = None;
            self.all_values_are_known = false;
        }
    }

    pub(crate) fn next_value<'db>(&self, db: &'db dyn Db) -> Type<'db> {
        if !self.has_values {
            return Type::int_literal(1);
        }
        let value = if Program::get(db).python_version(db) < PythonVersion::PY311 {
            self.last
        } else {
            self.maximum.filter(|_| self.all_values_are_known)
        };
        let Some(value) = value else {
            return KnownClass::Int.to_instance(db);
        };
        let bit_length = i64::BITS - value.unsigned_abs().leading_zeros();
        1_u64
            .checked_shl(bit_length)
            .and_then(|value| i64::try_from(value).ok())
            .map(Type::int_literal)
            .unwrap_or_else(|| KnownClass::Int.to_instance(db))
    }
}

/// Evaluate the integer value of a Flag member expression using values established by earlier
/// members in the same class body.
///
/// This is needed for expressions such as `READ_WRITE = READ | WRITE`: during ordinary class-body
/// inference, names assigned from `auto()` still have the placeholder `auto` type.
pub(crate) fn flag_member_expression_value<'db>(
    db: &'db dyn Db,
    definition: Definition<'db>,
    expression: &ast::Expr,
    previous_values: &FxHashMap<Name, i64>,
    auto_value: Option<i64>,
) -> (Option<i64>, bool) {
    fn evaluate<'db>(
        db: &'db dyn Db,
        definition: Definition<'db>,
        expression: &ast::Expr,
        previous_values: &FxHashMap<Name, i64>,
    ) -> Option<i64> {
        match expression {
            ast::Expr::Name(name) => previous_values.get(&name.id).copied().or_else(|| {
                definition_expression_type(db, definition, expression).as_int_like_literal()
            }),
            ast::Expr::BinOp(binary) => {
                let left = evaluate(db, definition, &binary.left, previous_values)?;
                let right = evaluate(db, definition, &binary.right, previous_values)?;
                match binary.op {
                    ast::Operator::BitOr => Some(left | right),
                    ast::Operator::BitAnd => Some(left & right),
                    ast::Operator::BitXor => Some(left ^ right),
                    _ => {
                        definition_expression_type(db, definition, expression).as_int_like_literal()
                    }
                }
            }
            ast::Expr::UnaryOp(unary) => {
                let value = evaluate(db, definition, &unary.operand, previous_values)?;
                match unary.op {
                    ast::UnaryOp::Invert => Some(!value),
                    ast::UnaryOp::UAdd => Some(value),
                    ast::UnaryOp::USub => value.checked_neg(),
                    ast::UnaryOp::Not => None,
                }
            }
            _ => definition_expression_type(db, definition, expression).as_int_like_literal(),
        }
    }

    let is_direct_auto = matches!(expression, ast::Expr::Call(_))
        && definition_expression_type(db, definition, expression)
            .is_instance_of(db, KnownClass::Auto);
    if is_direct_auto {
        return (auto_value, true);
    }

    (evaluate(db, definition, expression, previous_values), false)
}

/// Return the effective boundary inherited by a static `Flag` class.
pub(super) fn static_flag_boundary<'db>(
    db: &'db dyn Db,
    class: crate::types::StaticClassLiteral<'db>,
) -> FlagBoundary {
    for base in class
        .iter_mro(db, None)
        .filter_map(crate::types::ClassBase::into_class)
        .filter_map(|base| base.class_literal(db).as_static())
    {
        if !Type::ClassLiteral(ClassLiteral::Static(base))
            .is_subtype_of(db, KnownClass::Flag.to_subclass_of(db))
        {
            continue;
        }
        if let Some(boundary) = explicit_flag_boundary(db, base) {
            return boundary;
        }
        match base.known(db) {
            Some(KnownClass::IntFlag) => return FlagBoundary::Keep,
            Some(KnownClass::Flag) => return FlagBoundary::Strict,
            _ => {}
        }
    }
    FlagBoundary::Unknown
}

fn explicit_flag_boundary<'db>(
    db: &'db dyn Db,
    class: crate::types::StaticClassLiteral<'db>,
) -> Option<FlagBoundary> {
    if Program::get(db).python_version(db) < PythonVersion::PY311 {
        return None;
    }
    let module = parsed_module(db, class.file(db)).load(db);
    let definition = class.definition(db);
    let keyword = definition
        .kind(db)
        .as_class()?
        .node(&module)
        .arguments
        .as_ref()?
        .find_keyword("boundary")?;
    let ty = definition_expression_type(db, class.definition(db), &keyword.value);
    if ty.is_none(db) {
        None
    } else {
        Some(FlagBoundary::from_type(db, ty).unwrap_or(FlagBoundary::Unknown))
    }
}

fn flag_operand<'db>(
    db: &'db dyn Db,
    ty: Type<'db>,
) -> Option<(EnumClassLiteral<'db>, Option<EnumLiteralType<'db>>)> {
    match ty.resolve_type_alias(db) {
        Type::LiteralValue(literal) => {
            let enum_literal = literal.as_enum()?;
            let enum_class = enum_literal.enum_class_literal(db);
            enum_metadata_and_flag(db, enum_class)?;
            Some((enum_class, Some(enum_literal)))
        }
        Type::NominalInstance(instance) => {
            let enum_class = instance.class_literal(db).into_enum_class(db)?;
            enum_metadata_and_flag(db, enum_class)?;
            Some((enum_class, None))
        }
        Type::Intersection(intersection) => {
            let mut flags = intersection
                .positive(db)
                .iter()
                .filter_map(|positive| flag_operand(db, *positive));
            let (enum_class, mut literal) = flags.next()?;
            for (other_class, other_literal) in flags {
                if other_class != enum_class {
                    return None;
                }
                if literal != other_literal {
                    literal = None;
                }
            }
            Some((enum_class, literal))
        }
        _ => None,
    }
}

fn enum_metadata_and_flag<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
) -> Option<(&'db EnumMetadata<'db>, &'db FlagMetadata<'db>)> {
    let metadata = super::enum_metadata(db, enum_class.class_literal(db))?;
    Some((metadata, metadata.flag.as_ref()?))
}

fn literal_value<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    literal: EnumLiteralType<'db>,
) -> Option<i64> {
    let (metadata, flag) = enum_metadata_and_flag(db, enum_class)?;
    let name = metadata
        .aliases
        .get(literal.name(db))
        .unwrap_or(literal.name(db));
    flag.member_value(name)
}

fn type_for_flag_value<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    value: i64,
) -> Type<'db> {
    let Some((_, flag)) = enum_metadata_and_flag(db, enum_class) else {
        return enum_class.class_literal(db).to_non_generic_instance(db);
    };
    flag.named_member(value).map_or_else(
        || enum_class.class_literal(db).to_non_generic_instance(db),
        |name| Type::enum_literal(EnumLiteralType::new(db, enum_class, name.clone())),
    )
}

fn class_uses_standard_flag_method(
    db: &dyn Db,
    enum_class: EnumClassLiteral<'_>,
    name: &str,
) -> bool {
    let is_copied_onto_concrete_flag = matches!(
        name,
        "__or__" | "__and__" | "__xor__" | "__ror__" | "__rand__" | "__rxor__" | "__invert__"
    );
    match enum_class.class_literal(db) {
        ClassLiteral::Static(class) if is_copied_onto_concrete_flag => {
            custom_enum_method(db, class.body_scope(db), name).is_none()
        }
        ClassLiteral::Static(class) => !class
            .iter_mro(db, None)
            .filter_map(crate::types::ClassBase::into_class)
            .filter_map(|base| base.class_literal(db).as_static())
            .take_while(|base| {
                !matches!(base.known(db), Some(KnownClass::Flag | KnownClass::IntFlag))
            })
            .any(|base| custom_enum_method(db, base.body_scope(db), name).is_some()),
        ClassLiteral::DynamicEnum(_) if is_copied_onto_concrete_flag => true,
        ClassLiteral::DynamicEnum(enum_lit) => enum_lit.mixin_type(db).is_none_or(|mixin| {
            let Some(mixin) = mixin.to_class_type(db) else {
                return false;
            };
            !mixin
                .iter_mro(db)
                .filter_map(crate::types::ClassBase::into_class)
                .filter_map(|base| base.class_literal(db).as_static())
                .any(|base| custom_enum_method(db, base.body_scope(db), name).is_some())
        }),
        ClassLiteral::Dynamic(_)
        | ClassLiteral::DynamicNamedTuple(_)
        | ClassLiteral::DynamicTypedDict(_) => false,
    }
}

fn class_uses_standard_flag_construction(db: &dyn Db, enum_class: EnumClassLiteral<'_>) -> bool {
    class_uses_standard_flag_method(db, enum_class, "_missing_")
}

fn class_uses_standard_flag_operation(
    db: &dyn Db,
    enum_class: EnumClassLiteral<'_>,
    method: &str,
) -> bool {
    class_uses_standard_flag_method(db, enum_class, method)
        && class_uses_standard_flag_method(db, enum_class, "_get_value")
}

fn possible_flag_or_int<'db>(db: &'db dyn Db, enum_class: EnumClassLiteral<'db>) -> Type<'db> {
    UnionType::from_two_elements(
        db,
        enum_class.class_literal(db).to_non_generic_instance(db),
        KnownClass::Int.to_instance(db),
    )
}

fn construction_type<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    construction: FlagConstruction,
) -> Type<'db> {
    match construction {
        FlagConstruction::Flag(value) => type_for_flag_value(db, enum_class, value),
        FlagConstruction::Ejected(value) => Type::int_literal(value),
        FlagConstruction::Raises => enum_class.class_literal(db).to_non_generic_instance(db),
        FlagConstruction::Unknown => enum_metadata_and_flag(db, enum_class).map_or_else(
            || enum_class.class_literal(db).to_non_generic_instance(db),
            |(_, flag)| {
                if matches!(flag.boundary(), FlagBoundary::Eject | FlagBoundary::Unknown) {
                    possible_flag_or_int(db, enum_class)
                } else {
                    enum_class.class_literal(db).to_non_generic_instance(db)
                }
            },
        ),
    }
}

fn flag_integer_binary_result<'db>(
    db: &'db dyn Db,
    enum_class: EnumClassLiteral<'db>,
    literal: Option<EnumLiteralType<'db>>,
    operand: Type<'db>,
    standard_dispatch: bool,
    integer_value: Option<i64>,
    method: &str,
    operation: fn(i64, i64) -> i64,
) -> Option<Type<'db>> {
    let (_, flag) = enum_metadata_and_flag(db, enum_class)?;
    if !flag.accepts_operand(db, operand)
        || !standard_dispatch
        || !class_uses_standard_flag_operation(db, enum_class, method)
    {
        return None;
    }
    if !class_uses_standard_flag_construction(db, enum_class) {
        return Some(enum_class.class_literal(db).to_non_generic_instance(db));
    }

    let exact = literal
        .and_then(|literal| literal_value(db, enum_class, literal))
        .zip(integer_value)
        .map(|(left, right)| operation(left, right));

    Some(match exact {
        Some(value) => construction_type(db, enum_class, flag.construct(value)),
        None if matches!(flag.boundary(), FlagBoundary::Eject | FlagBoundary::Unknown) => {
            possible_flag_or_int(db, enum_class)
        }
        None => enum_class.class_literal(db).to_non_generic_instance(db),
    })
}

fn is_builtin_int_operand(db: &dyn Db, ty: Type<'_>) -> bool {
    match ty.resolve_type_alias(db) {
        Type::LiteralValue(literal) => literal.as_int().is_some(),
        Type::NominalInstance(instance) => instance.known_class(db) == Some(KnownClass::Int),
        _ => false,
    }
}

/// Infer a standard-library Flag bitwise operation without expanding possible combinations.
pub(crate) fn flag_binary_result<'db>(
    db: &'db dyn Db,
    left: Type<'db>,
    right: Type<'db>,
    op: ruff_python_ast::Operator,
) -> Option<Type<'db>> {
    let operation: fn(i64, i64) -> i64 = match op {
        ruff_python_ast::Operator::BitOr => |left, right| left | right,
        ruff_python_ast::Operator::BitAnd => |left, right| left & right,
        ruff_python_ast::Operator::BitXor => |left, right| left ^ right,
        _ => return None,
    };

    let left_flag = flag_operand(db, left);
    let right_flag = flag_operand(db, right);
    match (left_flag, right_flag) {
        (Some((left_class, left_literal)), Some((right_class, right_literal)))
            if left_class == right_class =>
        {
            if !class_uses_standard_flag_operation(db, left_class, op.dunder()) {
                return None;
            }
            if !class_uses_standard_flag_construction(db, left_class) {
                return Some(left_class.class_literal(db).to_non_generic_instance(db));
            }
            let value = left_literal.zip(right_literal).and_then(|(left, right)| {
                Some(operation(
                    literal_value(db, left_class, left)?,
                    literal_value(db, right_class, right)?,
                ))
            });
            let (_, flag) = enum_metadata_and_flag(db, left_class)?;
            Some(value.map_or_else(
                || left_class.class_literal(db).to_non_generic_instance(db),
                |value| construction_type(db, left_class, flag.construct(value)),
            ))
        }
        (Some((left_class, left_literal)), Some((right_class, right_literal))) => {
            flag_integer_binary_result(
                db,
                left_class,
                left_literal,
                right,
                true,
                right_literal.and_then(|literal| literal_value(db, right_class, literal)),
                op.dunder(),
                operation,
            )
        }
        (Some((enum_class, literal)), None) | (None, Some((enum_class, literal))) => {
            let flag_on_left = left_flag.is_some();
            let integer = if flag_on_left { right } else { left };
            let method = if flag_on_left {
                op.dunder()
            } else {
                op.reflected_dunder()
            };
            let standard_dispatch = flag_on_left || is_builtin_int_operand(db, integer);
            flag_integer_binary_result(
                db,
                enum_class,
                literal,
                integer,
                standard_dispatch,
                integer.as_int_like_literal(),
                method,
                operation,
            )
        }
        _ => None,
    }
}

/// Infer `~flag` for the standard `Flag.__invert__` implementation.
pub(crate) fn flag_invert_result<'db>(db: &'db dyn Db, operand: Type<'db>) -> Option<Type<'db>> {
    let (enum_class, literal) = flag_operand(db, operand)?;
    if !class_uses_standard_flag_operation(db, enum_class, "__invert__") {
        return None;
    }
    if !class_uses_standard_flag_construction(db, enum_class) {
        return Some(enum_class.class_literal(db).to_non_generic_instance(db));
    }
    let (_, flag) = enum_metadata_and_flag(db, enum_class)?;
    let Some(literal) = literal else {
        return Some(
            if matches!(flag.boundary(), FlagBoundary::Eject | FlagBoundary::Unknown) {
                possible_flag_or_int(db, enum_class)
            } else {
                enum_class.class_literal(db).to_non_generic_instance(db)
            },
        );
    };
    let value = literal_value(db, enum_class, literal)?;
    let construction = match flag.boundary() {
        FlagBoundary::Strict | FlagBoundary::Conform => flag
            .singles_mask
            .map(|mask| FlagConstruction::Flag(mask & !value))
            .unwrap_or(FlagConstruction::Unknown),
        FlagBoundary::Eject | FlagBoundary::Keep => flag.construct(!value),
        FlagBoundary::Unknown => FlagConstruction::Unknown,
    };
    Some(construction_type(db, enum_class, construction))
}

/// Adjust the return type of a call to a concrete `Flag` class for its boundary policy.
pub(crate) fn flag_constructor_result<'db>(
    db: &'db dyn Db,
    class: ClassLiteral<'db>,
    argument: Option<Type<'db>>,
) -> Option<Type<'db>> {
    let enum_class = class.into_enum_class(db)?;
    let (_, flag) = enum_metadata_and_flag(db, enum_class)?;
    if !enum_uses_standard_metaclass_call(db, class) {
        return None;
    }
    if !class_uses_standard_flag_construction(db, enum_class) {
        return Some(class.to_non_generic_instance(db));
    }
    if let Some((argument_class, literal)) = argument.and_then(|ty| flag_operand(db, ty))
        && argument_class == enum_class
    {
        return Some(literal.map_or_else(|| class.to_non_generic_instance(db), Type::enum_literal));
    }
    Some(match argument.and_then(Type::as_int_like_literal) {
        Some(value) => construction_type(db, enum_class, flag.construct(value)),
        None if matches!(flag.boundary(), FlagBoundary::Eject | FlagBoundary::Unknown) => {
            possible_flag_or_int(db, enum_class)
        }
        None => class.to_non_generic_instance(db),
    })
}

/// Return the truthiness of an exact Flag member when the standard `__bool__` is in use.
pub(crate) fn flag_literal_truthiness(db: &dyn Db, literal: EnumLiteralType<'_>) -> Option<bool> {
    let enum_class = literal.enum_class_literal(db);
    class_uses_standard_flag_method(db, enum_class, "__bool__")
        .then(|| literal_value(db, enum_class, literal).map(|value| value != 0))
        .flatten()
}

/// Return the length of an exact Flag member when the standard `__len__` is in use.
pub(crate) fn flag_literal_len(db: &dyn Db, literal: EnumLiteralType<'_>) -> Option<i64> {
    if Program::get(db).python_version(db) < PythonVersion::PY311 {
        return None;
    }
    let enum_class = literal.enum_class_literal(db);
    class_uses_standard_flag_method(db, enum_class, "__len__")
        .then(|| {
            literal_value(db, enum_class, literal)
                .map(|value| i64::from(value.unsigned_abs().count_ones()))
        })
        .flatten()
}

/// Return the constituent canonical members yielded by iterating an exact Flag member.
pub(crate) fn flag_literal_iteration<'db>(
    db: &'db dyn Db,
    literal: EnumLiteralType<'db>,
) -> Option<FlagIteration<'db>> {
    if Program::get(db).python_version(db) < PythonVersion::PY311 {
        return None;
    }
    let enum_class = literal.enum_class_literal(db);
    if !class_uses_standard_flag_method(db, enum_class, "__iter__") {
        return None;
    }
    let (_, flag) = enum_metadata_and_flag(db, enum_class)?;
    let nominal = enum_class.class_literal(db).to_non_generic_instance(db);
    if !class_uses_standard_flag_method(db, enum_class, "_iter_member_")
        || !flag.canonical_members_are_known()
    {
        return Some(FlagIteration::Unknown(nominal));
    }
    let Some(value) = literal_value(db, enum_class, literal) else {
        return Some(FlagIteration::Unknown(nominal));
    };
    if value < 0 {
        return Some(FlagIteration::Unknown(nominal));
    }
    Some(FlagIteration::Exact(
        flag.canonical_members()
            .iter()
            .filter(|(_, member)| value & member != 0)
            .map(|(name, _)| name)
            .map(|name| Type::enum_literal(EnumLiteralType::new(db, enum_class, name.clone())))
            .collect(),
    ))
}

pub(crate) enum FlagIteration<'db> {
    Exact(Vec<Type<'db>>),
    Unknown(Type<'db>),
}

const fn is_positive_power_of_two(value: i64) -> bool {
    value > 0 && value & (value - 1) == 0
}

/// Evaluate exact Flag subset membership (`member in flags`).
pub(crate) fn flag_membership_result(
    db: &dyn Db,
    member: Type<'_>,
    flags: Type<'_>,
) -> Option<bool> {
    let (member_class, Some(member)) = flag_operand(db, member)? else {
        return None;
    };
    let (flags_class, Some(flags)) = flag_operand(db, flags)? else {
        return None;
    };
    if member_class != flags_class
        || !class_uses_standard_flag_method(db, flags_class, "__contains__")
    {
        return None;
    }
    let member = literal_value(db, member_class, member)?;
    let flags = literal_value(db, flags_class, flags)?;
    Some(flags & member == member)
}
