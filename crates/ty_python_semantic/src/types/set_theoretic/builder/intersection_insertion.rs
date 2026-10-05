//! Signed inner intersection insertion with flat pending additions.

use std::convert::Infallible;

use smallvec::{SmallVec, smallvec};
use ty_mapping_probe_macros::shared_semantic_family;

#[cfg(any(test, feature = "experimental-analysis"))]
use super::intersection_storage::SignedSetStorage;
use super::{
    InnerIntersectionBuilder, IntersectionPolarity, IntersectionSimplification,
    simplify_intersection_pair,
};
use crate::types::set_theoretic::NegativeIntersectionElementsIterator;
use crate::types::set_theoretic::generic_gradual_intersections::{
    GenericIntersection, generic_gradual_intersection,
};
use crate::types::{
    ClassLiteral, EnumLiteralType, IntersectionType, KnownClass, LiteralValueType,
    LiteralValueTypeKind, NominalInstanceType, SubclassOfType, Type, TypeFormType,
};
use crate::{Db, ProgramEnvironment};

#[derive(Clone, Copy)]
pub(in crate::types) enum Sign {
    Positive,
    Negative,
}

pub(in crate::types) enum Elements<'db> {
    Positive(ordermap::set::Iter<'db, Type<'db>>),
    Negative(NegativeIntersectionElementsIterator<'db, 'db>),
}

pub(in crate::types) enum Frame<'db> {
    Add(Type<'db>, Sign),
    // A preceding addition finishes before constructing this literal.
    EmptyString(Sign),
    Sequence {
        elements: Elements<'db>,
        sign: Sign,
        next_negative: Option<IntersectionType<'db>>,
    },
}

pub(in crate::types) struct Insertion<'a, 'db> {
    builder: &'a mut InnerIntersectionBuilder<'db>,
    frames: SmallVec<[Frame<'db>; 2]>,
    removals: SmallVec<[usize; 1]>,
    #[cfg(test)]
    _lifetime: insertion_observations::OwnerLifetime,
}

impl<'a, 'db> Insertion<'a, 'db> {
    pub(in crate::types) fn new(
        builder: &'a mut InnerIntersectionBuilder<'db>,
        initial: Frame<'db>,
    ) -> Self {
        Self {
            builder,
            frames: smallvec![initial],
            removals: SmallVec::new(),
            #[cfg(test)]
            _lifetime: insertion_observations::entered(),
        }
    }

    pub(in crate::types) fn next_frame(&mut self) -> Option<Frame<'db>> {
        self.frames.pop()
    }

    pub(in crate::types) fn push_frame(&mut self, frame: Frame<'db>) {
        self.frames.push(frame);
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn frames_storage(&self) -> (usize, usize, bool) {
        (
            self.frames.len(),
            self.frames.capacity(),
            self.frames.spilled(),
        )
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn removals_storage(&self) -> (usize, usize, bool) {
        (
            self.removals.len(),
            self.removals.capacity(),
            self.removals.spilled(),
        )
    }

    #[cfg(any(test, feature = "experimental-analysis"))]
    pub(in crate::types) fn signed_storage(&self, sign: Sign) -> SignedSetStorage {
        self.builder.signed_storage(sign)
    }

    pub(in crate::types) fn contains_signed(&self, sign: Sign, ty: Type<'db>) -> bool {
        self.builder.contains_signed(sign, ty)
    }

    pub(in crate::types) fn insert_signed(&mut self, sign: Sign, ty: Type<'db>) {
        self.builder.insert_signed(sign, ty);
    }

    pub(in crate::types) fn remove_signed(&mut self, sign: Sign, ty: Type<'db>) -> bool {
        self.builder.remove_signed(sign, ty)
    }

    pub(in crate::types) fn remove_signed_index(&mut self, sign: Sign, index: usize) {
        self.builder.remove_signed_index(sign, index);
    }

    pub(in crate::types) fn reset_to(&mut self, ty: Type<'db>) {
        self.builder.reset_to(ty);
    }

    pub(in crate::types) fn next_signed(
        &self,
        sign: Sign,
        cursor: &mut usize,
    ) -> Option<(usize, Type<'db>)> {
        self.builder.next_signed(sign, cursor)
    }

    pub(in crate::types) fn positive_len(&self) -> usize {
        self.builder.positive.len()
    }

    pub(in crate::types) fn clear_removals(&mut self) {
        self.removals.clear();
    }

    pub(in crate::types) fn defer_removal(&mut self, index: usize) {
        self.removals.push(index);
    }

    pub(in crate::types) fn next_removal(&mut self) -> Option<usize> {
        self.removals.pop()
    }
}

#[cfg(test)]
pub(in crate::types) mod insertion_observations {
    use std::cell::Cell;

    thread_local! {
        static LIVE: Cell<usize> = const { Cell::new(0) };
        static ENTERED: Cell<usize> = const { Cell::new(0) };
    }

    pub(in crate::types) fn reset() {
        assert_eq!(LIVE.get(), 0);
        ENTERED.set(0);
    }

    pub(in crate::types) fn progress() -> (usize, usize) {
        (LIVE.get(), ENTERED.get())
    }

    pub(super) struct OwnerLifetime;

    impl Drop for OwnerLifetime {
        fn drop(&mut self) {
            LIVE.set(LIVE.get() - 1);
        }
    }

    pub(super) fn entered() -> OwnerLifetime {
        LIVE.set(LIVE.get() + 1);
        ENTERED.set(ENTERED.get() + 1);
        OwnerLifetime
    }
}

pub(in crate::types) struct InsertionFacts;

pub(in crate::types) struct OrdinaryInsertionEffects<'a, 'db> {
    db: &'db dyn Db,
    env: &'a ProgramEnvironment<'db>,
}

impl<'a, 'db> OrdinaryInsertionEffects<'a, 'db> {
    pub(in crate::types) fn new(db: &'db dyn Db, env: &'a ProgramEnvironment<'db>) -> Self {
        Self { db, env }
    }
}

shared_semantic_family! {
    #[synchronous(SynchronousInsertionEffects)]
    // Controlled implementations retain owned arguments outside rejectable callbacks and
    // transfer them only after admission. Frame and removal storage prepay disposal on refusal.
    pub(in crate::types) trait InsertionEffects<'db> {
        type Error;

        #[operation(local)]
        async fn start<'a>(&self, builder: &'a mut InnerIntersectionBuilder<'db>, initial: Frame<'db>) -> Result<Insertion<'a, 'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next(&self, insertion: &mut Insertion<'_, 'db>) -> Result<Option<Frame<'db>>, Self::Error>;
        #[operation(local)]
        async fn push(&self, insertion: &mut Insertion<'_, 'db>, frame: Frame<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn finish(&self, insertion: Insertion<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn contains(&self, insertion: &Insertion<'_, 'db>, sign: Sign, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn remove(&self, insertion: &mut Insertion<'_, 'db>, sign: Sign, ty: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(local)]
        async fn remove_index(&self, insertion: &mut Insertion<'_, 'db>, sign: Sign, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn insert(&self, insertion: &mut Insertion<'_, 'db>, sign: Sign, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn reset(&self, insertion: &mut Insertion<'_, 'db>, ty: Type<'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_stored(&self, insertion: &Insertion<'_, 'db>, sign: Sign, cursor: &mut usize) -> Result<Option<(usize, Type<'db>)>, Self::Error>;
        #[operation(local)]
        async fn assert_divergent_alone(&self, insertion: &Insertion<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn clear_removals(&self, insertion: &mut Insertion<'_, 'db>) -> Result<(), Self::Error>;
        #[operation(local)]
        async fn defer_removal(&self, insertion: &mut Insertion<'_, 'db>, index: usize) -> Result<(), Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_removal(&self, insertion: &mut Insertion<'_, 'db>) -> Result<Option<usize>, Self::Error>;
        #[operation(source)]
        async fn positive_elements(&self, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(source)]
        async fn negative_elements(&self, intersection: IntersectionType<'db>) -> Result<Elements<'db>, Self::Error>;
        #[operation(local)]
        #[progress]
        async fn next_element(&self, elements: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn empty_string(&self) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn typeform_argument(&self, typeform: TypeFormType<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn subclass_from_instance(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error>;
        #[operation(source)]
        async fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error>;
        #[operation(source)]
        async fn has_known_class(&self, instance: NominalInstanceType<'db>, class: KnownClass) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn known_class(&self, instance: NominalInstanceType<'db>) -> Result<Option<KnownClass>, Self::Error>;
        #[operation(source)]
        async fn enum_class(&self, literal: EnumLiteralType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
        #[operation(source)]
        async fn instance_class(&self, instance: NominalInstanceType<'db>) -> Result<ClassLiteral<'db>, Self::Error>;
        #[operation(local)]
        async fn types_equal(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error>;
        #[operation(source)]
        async fn generic_intersection(&self, first: Type<'db>, second: Type<'db>) -> Result<Option<GenericIntersection<'db>>, Self::Error>;
        #[operation(source)]
        async fn simplify_pair(&self, first: Type<'db>, second: Type<'db>, polarity: IntersectionPolarity) -> Result<IntersectionSimplification, Self::Error>;
    }

    #[finite_capability]
    impl InsertionFacts {
        fn is_never(&self, ty: Type<'_>) -> bool { ty.is_never() }
        fn is_divergent(&self, ty: Type<'_>) -> bool { ty.is_divergent() }
        fn negated_divergent<'db>(&self, ty: Type<'db>) -> Option<Type<'db>> { ty.negated_divergent() }
        fn is_object(&self, instance: NominalInstanceType<'_>) -> bool { instance.is_object() }
        fn as_nominal_instance<'db>(&self, ty: Type<'db>) -> Option<NominalInstanceType<'db>> { ty.as_nominal_instance() }
        fn as_enum_literal<'db>(&self, ty: Type<'db>) -> Option<EnumLiteralType<'db>> { ty.as_enum_literal() }
        fn literal_string<'db>(&self) -> Type<'db> { Type::literal_string() }
        fn is_literal_string(&self, literal: LiteralValueType<'_>) -> bool { literal.is_literal_string() }
        fn literal_kind<'db>(&self, literal: LiteralValueType<'db>) -> LiteralValueTypeKind<'db> { literal.kind() }
        fn literal_is_bool(&self, literal: LiteralValueType<'_>, value: bool) -> bool { literal.as_bool() == Some(value) }
        fn bool_literal<'db>(&self, value: bool) -> Type<'db> { Type::bool_literal(value) }
        fn needs_bool_scan(&self, ty: Type<'_>) -> bool {
            matches!(ty, Type::AlwaysTruthy | Type::AlwaysFalsy)
                || matches!(ty, Type::LiteralValue(literal) if literal.as_bool().is_some())
        }
        fn is_bool_class(&self, class: Option<KnownClass>) -> bool { class.is_some_and(KnownClass::is_bool) }
        fn same_class(&self, first: ClassLiteral<'_>, second: ClassLiteral<'_>) -> bool { first == second }
        fn same_enum(&self, first: EnumLiteralType<'_>, second: EnumLiteralType<'_>) -> bool { first == second }
    }

    #[synchronous(add_sync)]
    #[capabilities(effects = InsertionEffects, facts = InsertionFacts)]
    #[passive_values(Frame::Add, Frame::EmptyString, Frame::Sequence, Sign::Positive, Sign::Negative, Type::Never, Type::AlwaysTruthy, Type::AlwaysFalsy, KnownClass::Type, KnownClass::Bool, IntersectionPolarity::Positive, IntersectionPolarity::Negative, IntersectionPolarity::Mixed)]
    pub(in crate::types) async fn add_with<'a, 'db, E: InsertionEffects<'db>>(
        builder: &'a mut InnerIntersectionBuilder<'db>, ty: Type<'db>, sign: Sign,
        facts: InsertionFacts, effects: &E,
    ) -> Result<(), E::Error> {
        let mut insertion = effects.start(builder, Frame::Add(ty, sign)).await?;
        #[cursor_loop]
        while let Some(frame) = effects.next(&mut insertion).await? {
            match frame {
                Frame::Add(ty, Sign::Positive) => {
                    #[passive_state]
                    let mut new_positive = ty;
                    // `Never & T` -> `Never`
                    if effects.contains(&insertion, Sign::Positive, Type::Never).await? {
                        continue;
                    }

                    // `T & Never` -> `Never`
                    if facts.is_never(new_positive) {
                        effects.reset(&mut insertion, Type::Never).await?;
                        continue;
                    }

                    // `T & Divergent` -> `Divergent`. Conceptually, `Divergent` behaves like `Never` here and
                    // dominates intersections. However, `Divergent` is actually a dynamic/gradual type, so
                    // `~Divergent` acts like `Divergent` rather than dropping out like `~Never` does.
                    // `Divergent` also gets a lot of special handling in cycle recovery.
                    if facts.is_divergent(new_positive) {
                        effects.reset(&mut insertion, new_positive).await?;
                        continue;
                    }
                    // `Divergent & T` -> `Divergent`
                    let mut cursor = 0;
                    #[passive_state]
                    let mut contains_divergent = false;
                    #[cursor_loop]
                    while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                        let (_, existing) = entry;
                        if facts.is_divergent(existing) {
                            contains_divergent = true;
                            break;
                        }
                    }
                    if contains_divergent {
                        continue;
                    }

                    // A runtime class value of `TypeForm[T]` has type `type[T]`.
                    match new_positive {
                        Type::TypeForm(typeform) => {
                            let argument = effects.typeform_argument(typeform).await?;
                            let argument = effects.resolve_alias(argument).await?;
                            if let Some(narrowed) = effects.subclass_from_instance(argument).await? {
                                let type_instance = effects.known_instance(KnownClass::Type).await?;
                                if effects.remove(&mut insertion, Sign::Positive, type_instance).await? {
                                    new_positive = narrowed;
                                }
                            }
                        }
                        Type::NominalInstance(instance) if effects.has_known_class(instance, KnownClass::Type).await? => {
                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                                let (index, positive) = entry;
                                if let Type::TypeForm(typeform) = positive {
                                    let argument = effects.typeform_argument(typeform).await?;
                                    let argument = effects.resolve_alias(argument).await?;
                                    if let Some(narrowed) = effects.subclass_from_instance(argument).await? {
                                        effects.remove_index(&mut insertion, Sign::Positive, index).await?;
                                        new_positive = narrowed;
                                        break;
                                    }
                                }
                            }
                        }
                        _ => {}
                    }

                    match new_positive {
                        // `LiteralString & AlwaysTruthy` -> `LiteralString & ~Literal[""]`
                        Type::AlwaysTruthy if effects.contains(&insertion, Sign::Positive, facts.literal_string()).await? => {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Negative)).await?;
                        }
                        // `LiteralString & AlwaysFalsy` -> `Literal[""]`
                        Type::AlwaysFalsy if effects.remove(&mut insertion, Sign::Positive, facts.literal_string()).await? => {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Positive)).await?;
                        }
                        // `AlwaysTruthy & LiteralString` -> `LiteralString & ~Literal[""]`
                        Type::LiteralValue(literal)
                            if facts.is_literal_string(literal)
                                && effects.remove(&mut insertion, Sign::Positive, Type::AlwaysTruthy).await? =>
                        {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Negative)).await?;
                            effects.push(&mut insertion, Frame::Add(facts.literal_string(), Sign::Positive)).await?;
                        }
                        // `AlwaysFalsy & LiteralString` -> `Literal[""]`
                        Type::LiteralValue(literal)
                            if facts.is_literal_string(literal) && effects.remove(&mut insertion, Sign::Positive, Type::AlwaysFalsy).await? =>
                        {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Positive)).await?;
                        }
                        // `LiteralString & ~AlwaysTruthy` -> `LiteralString & AlwaysFalsy` -> `Literal[""]`
                        Type::LiteralValue(literal)
                            if facts.is_literal_string(literal)
                                && effects.remove(&mut insertion, Sign::Negative, Type::AlwaysTruthy).await? =>
                        {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Positive)).await?;
                        }
                        // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
                        Type::LiteralValue(literal)
                            if facts.is_literal_string(literal) && effects.remove(&mut insertion, Sign::Negative, Type::AlwaysFalsy).await? =>
                        {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Negative)).await?;
                            effects.push(&mut insertion, Frame::Add(facts.literal_string(), Sign::Positive)).await?;
                        }

                        _ => {
                            let positive_as_instance = facts.as_nominal_instance(new_positive);

                            if let Some(instance) = positive_as_instance
                                && facts.is_object(instance)
                            {
                                // `object & T` -> `T`; it is always redundant to add `object` to an intersection
                                continue;
                            }

                            let addition_is_bool_instance = if let Some(instance) = positive_as_instance {
                                effects.has_known_class(instance, KnownClass::Bool).await?
                            } else {
                                false
                            };

                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                                let (index, existing_positive) = entry;
                                match existing_positive {
                                    // `AlwaysTruthy & bool` -> `Literal[True]`
                                    Type::AlwaysTruthy if addition_is_bool_instance => {
                                        new_positive = facts.bool_literal(true);
                                    }
                                    // `AlwaysFalsy & bool` -> `Literal[False]`
                                    Type::AlwaysFalsy if addition_is_bool_instance => {
                                        new_positive = facts.bool_literal(false);
                                    }
                                    Type::NominalInstance(instance)
                                        if effects.has_known_class(instance, KnownClass::Bool).await? =>
                                    {
                                        match new_positive {
                                            // `bool & AlwaysTruthy` -> `Literal[True]`
                                            Type::AlwaysTruthy => {
                                                new_positive = facts.bool_literal(true);
                                            }
                                            // `bool & AlwaysFalsy` -> `Literal[False]`
                                            Type::AlwaysFalsy => {
                                                new_positive = facts.bool_literal(false);
                                            }
                                            _ => continue,
                                        }
                                    }
                                    _ => continue,
                                }
                                effects.remove_index(&mut insertion, Sign::Positive, index).await?;
                                break;
                            }

                            if addition_is_bool_instance {
                                let mut cursor = 0;
                                #[cursor_loop]
                                while let Some(entry) = effects.next_stored(&insertion, Sign::Negative, &mut cursor).await? {
                                    let (index, existing_negative) = entry;
                                    match existing_negative {
                                        // `bool & ~Literal[False]` -> `Literal[True]`
                                        // `bool & ~Literal[True]` -> `Literal[False]`
                                        Type::LiteralValue(literal) => match facts.literal_kind(literal) {
                                            LiteralValueTypeKind::Bool(bool_value) => {
                                                new_positive = facts.bool_literal(!bool_value);
                                            }
                                            _ => continue,
                                        },
                                        // `bool & ~AlwaysTruthy` -> `Literal[False]`
                                        Type::AlwaysTruthy => {
                                            new_positive = facts.bool_literal(false);
                                        }
                                        // `bool & ~AlwaysFalsy` -> `Literal[True]`
                                        Type::AlwaysFalsy => {
                                            new_positive = facts.bool_literal(true);
                                        }
                                        _ => continue,
                                    }
                                    effects.remove_index(&mut insertion, Sign::Negative, index).await?;
                                    break;
                                }
                            }

                            effects.clear_removals(&mut insertion).await?;
                            #[passive_state]
                            let mut finished = false;
                            #[passive_state]
                            let mut replacement = None;
                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                                let (index, existing_positive) = entry;
                                if let Some(result) =
                                    effects.generic_intersection(new_positive, existing_positive).await?
                                {
                                    let GenericIntersection::Simplified(merged) = result else {
                                        continue;
                                    };
                                    if effects.types_equal(merged, existing_positive).await? {
                                        finished = true;
                                        break;
                                    }
                                    replacement = Some((index, merged));
                                    break;
                                }
                                match effects.simplify_pair(existing_positive, new_positive, IntersectionPolarity::Positive).await? {
                                    IntersectionSimplification::Unchanged => {}
                                    IntersectionSimplification::SecondRedundant => {
                                        finished = true;
                                        break;
                                    }
                                    IntersectionSimplification::FirstRedundant => effects.defer_removal(&mut insertion, index).await?,
                                    IntersectionSimplification::Disjoint => {
                                        effects.reset(&mut insertion, Type::Never).await?;
                                        finished = true;
                                        break;
                                    }
                                }
                            }
                            if finished {
                                effects.clear_removals(&mut insertion).await?;
                                continue;
                            }
                            if let Some((index, value)) = replacement {
                                effects.remove_index(&mut insertion, Sign::Positive, index).await?;
                                effects.clear_removals(&mut insertion).await?;
                                effects.push(&mut insertion, Frame::Add(value, Sign::Positive)).await?;
                                continue;
                            }
                            #[cursor_loop]
                            while let Some(index) = effects.next_removal(&mut insertion).await? {
                                effects.remove_index(&mut insertion, Sign::Positive, index).await?;
                            }

                            effects.clear_removals(&mut insertion).await?;
                            #[passive_state]
                            let mut finished = false;
                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(entry) = effects.next_stored(&insertion, Sign::Negative, &mut cursor).await? {
                                let (index, existing_negative) = entry;
                                match effects.simplify_pair(new_positive, existing_negative, IntersectionPolarity::Mixed).await? {
                                    IntersectionSimplification::Unchanged => {}
                                    IntersectionSimplification::SecondRedundant => effects.defer_removal(&mut insertion, index).await?,
                                    IntersectionSimplification::FirstRedundant => {
                                        finished = true;
                                        break;
                                    }
                                    IntersectionSimplification::Disjoint => {
                                        effects.reset(&mut insertion, Type::Never).await?;
                                        finished = true;
                                        break;
                                    }
                                }
                            }
                            if finished {
                                effects.clear_removals(&mut insertion).await?;
                                continue;
                            }
                            #[cursor_loop]
                            while let Some(index) = effects.next_removal(&mut insertion).await? {
                                effects.remove_index(&mut insertion, Sign::Negative, index).await?;
                            }

                            effects.insert(&mut insertion, Sign::Positive, new_positive).await?;
                        }
                    }
                }
                Frame::Add(new_negative, Sign::Negative) => {
                    // `Never & ~T` -> `Never`.
                    if effects.contains(&insertion, Sign::Positive, Type::Never).await? {
                        continue;
                    }

                    // `Divergent & ~T` -> `Divergent`.
                    let mut cursor = 0;
                    #[passive_state]
                    let mut contains_divergent = false;
                    #[cursor_loop]
                    while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                        let (_, existing) = entry;
                        if facts.is_divergent(existing) {
                            contains_divergent = true;
                            break;
                        }
                    }
                    if contains_divergent {
                        effects.assert_divergent_alone(&insertion).await?;
                        continue;
                    }

                    if let Some(negated_divergent) = facts.negated_divergent(new_negative) {
                        effects.reset(&mut insertion, negated_divergent).await?;
                        continue;
                    }

                    #[passive_state]
                    let mut contains_bool = false;
                    if facts.needs_bool_scan(new_negative) {
                        let mut cursor = 0;
                        #[cursor_loop]
                        while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                            let (_, positive) = entry;
                            if let Some(instance) = facts.as_nominal_instance(positive) {
                                let known_class = effects.known_class(instance).await?;
                                if facts.is_bool_class(known_class) {
                                    contains_bool = true;
                                    break;
                                }
                            }
                        }
                    }

                    match new_negative {
                        Type::Intersection(inter) => {
                            let elements = effects.positive_elements(inter).await?;
                            effects.push(&mut insertion, Frame::Sequence {
                                elements, sign: Sign::Negative, next_negative: Some(inter),
                            }).await?;
                        }
                        Type::Never => {
                            // Adding ~Never to an intersection is a no-op.
                        }
                        Type::NominalInstance(instance) if facts.is_object(instance) => {
                            // Adding ~object to an intersection results in Never.
                            effects.reset(&mut insertion, Type::Never).await?;
                        }
                        ty @ Type::Dynamic(_) => {
                            // Adding any of these types to the negative side of an intersection
                            // is equivalent to adding it to the positive side. We do this to
                            // simplify the representation.
                            effects.push(&mut insertion, Frame::Add(ty, Sign::Positive)).await?;
                        }
                        // `bool & ~AlwaysTruthy` -> `bool & Literal[False]`
                        Type::AlwaysTruthy if contains_bool => {
                            effects.push(&mut insertion, Frame::Add(facts.bool_literal(false), Sign::Positive)).await?;
                        }
                        // `bool & ~Literal[True]` -> `bool & Literal[False]`
                        Type::LiteralValue(literal) if facts.literal_is_bool(literal, true) && contains_bool => {
                            effects.push(&mut insertion, Frame::Add(facts.bool_literal(false), Sign::Positive)).await?;
                        }
                        // `LiteralString & ~AlwaysTruthy` -> `LiteralString & Literal[""]`
                        Type::AlwaysTruthy if effects.contains(&insertion, Sign::Positive, facts.literal_string()).await? => {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Positive)).await?;
                        }
                        // `bool & ~AlwaysFalsy` -> `bool & Literal[True]`
                        Type::AlwaysFalsy if contains_bool => {
                            effects.push(&mut insertion, Frame::Add(facts.bool_literal(true), Sign::Positive)).await?;
                        }
                        // `bool & ~Literal[False]` -> `bool & Literal[True]`
                        Type::LiteralValue(literal) if facts.literal_is_bool(literal, false) && contains_bool => {
                            effects.push(&mut insertion, Frame::Add(facts.bool_literal(true), Sign::Positive)).await?;
                        }
                        // `LiteralString & ~AlwaysFalsy` -> `LiteralString & ~Literal[""]`
                        Type::AlwaysFalsy if effects.contains(&insertion, Sign::Positive, facts.literal_string()).await? => {
                            effects.push(&mut insertion, Frame::EmptyString(Sign::Negative)).await?;
                        }
                        _ => {
                            let new_negative_enum = facts.as_enum_literal(new_negative);
                            effects.clear_removals(&mut insertion).await?;
                            #[passive_state]
                            let mut finished = false;
                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(entry) = effects.next_stored(&insertion, Sign::Negative, &mut cursor).await? {
                                let (index, existing_negative) = entry;
                                if let Some(new_enum) = new_negative_enum
                                    && let Some(existing_enum) = facts.as_enum_literal(existing_negative)
                                {
                                    let existing_class = effects.enum_class(existing_enum).await?;
                                    let new_class = effects.enum_class(new_enum).await?;
                                    if facts.same_class(existing_class, new_class) {
                                        if facts.same_enum(existing_enum, new_enum) {
                                            finished = true;
                                            break;
                                        }
                                        continue;
                                    }
                                }

                                match effects.simplify_pair(existing_negative, new_negative, IntersectionPolarity::Negative).await? {
                                    IntersectionSimplification::Unchanged => {}
                                    IntersectionSimplification::SecondRedundant => {
                                        finished = true;
                                        break;
                                    }
                                    IntersectionSimplification::FirstRedundant => effects.defer_removal(&mut insertion, index).await?,
                                    IntersectionSimplification::Disjoint => {
                                        effects.reset(&mut insertion, Type::Never).await?;
                                        finished = true;
                                        break;
                                    }
                                }
                            }
                            if finished {
                                effects.clear_removals(&mut insertion).await?;
                                continue;
                            }
                            #[cursor_loop]
                            while let Some(index) = effects.next_removal(&mut insertion).await? {
                                effects.remove_index(&mut insertion, Sign::Negative, index).await?;
                            }

                            effects.clear_removals(&mut insertion).await?;
                            #[passive_state]
                            let mut finished = false;
                            let mut cursor = 0;
                            #[cursor_loop]
                            while let Some(entry) = effects.next_stored(&insertion, Sign::Positive, &mut cursor).await? {
                                let (index, existing_positive) = entry;
                                if let Some(new_enum) = new_negative_enum {
                                    if let Some(existing_enum) = facts.as_enum_literal(existing_positive) {
                                        let existing_class = effects.enum_class(existing_enum).await?;
                                        let new_class = effects.enum_class(new_enum).await?;
                                        if facts.same_class(existing_class, new_class) {
                                            if facts.same_enum(existing_enum, new_enum) {
                                                effects.reset(&mut insertion, Type::Never).await?;
                                            }
                                            finished = true;
                                            break;
                                        }
                                    }

                                    if let Some(instance) = facts.as_nominal_instance(existing_positive) {
                                        let existing_class = effects.instance_class(instance).await?;
                                        let new_class = effects.enum_class(new_enum).await?;
                                        if facts.same_class(existing_class, new_class) {
                                            continue;
                                        }
                                    }
                                }

                                match effects.simplify_pair(existing_positive, new_negative, IntersectionPolarity::Mixed).await? {
                                    IntersectionSimplification::Unchanged => {}
                                    IntersectionSimplification::SecondRedundant => {
                                        finished = true;
                                        break;
                                    }
                                    IntersectionSimplification::FirstRedundant => effects.defer_removal(&mut insertion, index).await?,
                                    IntersectionSimplification::Disjoint => {
                                        effects.reset(&mut insertion, Type::Never).await?;
                                        finished = true;
                                        break;
                                    }
                                }
                            }

                            if finished {
                                effects.clear_removals(&mut insertion).await?;
                                continue;
                            }
                            #[cursor_loop]
                            while let Some(index) = effects.next_removal(&mut insertion).await? {
                                effects.remove_index(&mut insertion, Sign::Positive, index).await?;
                            }

                            effects.insert(&mut insertion, Sign::Negative, new_negative).await?;
                        }
                    }
                }
                Frame::EmptyString(sign) => {
                    let ty = effects.empty_string().await?;
                    effects.push(&mut insertion, Frame::Add(ty, sign)).await?;
                }
                Frame::Sequence { mut elements, sign, next_negative } => {
                    if let Some(ty) = effects.next_element(&mut elements).await? {
                        effects.push(&mut insertion, Frame::Sequence { elements, sign, next_negative }).await?;
                        effects.push(&mut insertion, Frame::Add(ty, sign)).await?;
                    } else if let Some(intersection) = next_negative {
                        let elements = effects.negative_elements(intersection).await?;
                        effects.push(&mut insertion, Frame::Sequence {
                            elements, sign: Sign::Positive, next_negative: None,
                        }).await?;
                    }
                }
            }
        }
        effects.finish(insertion).await
    }
}

impl<'db> SynchronousInsertionEffects<'db> for OrdinaryInsertionEffects<'_, 'db> {
    type Error = Infallible;

    fn start<'a>(
        &self,
        builder: &'a mut InnerIntersectionBuilder<'db>,
        initial: Frame<'db>,
    ) -> Result<Insertion<'a, 'db>, Self::Error> {
        Ok(Insertion::new(builder, initial))
    }

    fn next(&self, insertion: &mut Insertion<'_, 'db>) -> Result<Option<Frame<'db>>, Self::Error> {
        Ok(insertion.next_frame())
    }

    fn push(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        frame: Frame<'db>,
    ) -> Result<(), Self::Error> {
        insertion.push_frame(frame);
        Ok(())
    }

    fn finish(&self, insertion: Insertion<'_, 'db>) -> Result<(), Self::Error> {
        drop(insertion);
        Ok(())
    }

    fn contains(
        &self,
        insertion: &Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(insertion.contains_signed(sign, ty))
    }

    fn remove(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> Result<bool, Self::Error> {
        Ok(insertion.remove_signed(sign, ty))
    }

    fn remove_index(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        sign: Sign,
        index: usize,
    ) -> Result<(), Self::Error> {
        insertion.remove_signed_index(sign, index);
        Ok(())
    }

    fn insert(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        sign: Sign,
        ty: Type<'db>,
    ) -> Result<(), Self::Error> {
        insertion.insert_signed(sign, ty);
        Ok(())
    }

    fn reset(&self, insertion: &mut Insertion<'_, 'db>, ty: Type<'db>) -> Result<(), Self::Error> {
        insertion.reset_to(ty);
        Ok(())
    }

    fn next_stored(
        &self,
        insertion: &Insertion<'_, 'db>,
        sign: Sign,
        cursor: &mut usize,
    ) -> Result<Option<(usize, Type<'db>)>, Self::Error> {
        Ok(insertion.next_signed(sign, cursor))
    }

    fn assert_divergent_alone(&self, insertion: &Insertion<'_, 'db>) -> Result<(), Self::Error> {
        debug_assert_eq!(
            insertion.positive_len(),
            1,
            "`Divergent` should be alone"
        );
        Ok(())
    }

    fn clear_removals(&self, insertion: &mut Insertion<'_, 'db>) -> Result<(), Self::Error> {
        insertion.clear_removals();
        Ok(())
    }

    fn defer_removal(
        &self,
        insertion: &mut Insertion<'_, 'db>,
        index: usize,
    ) -> Result<(), Self::Error> {
        insertion.defer_removal(index);
        Ok(())
    }

    fn next_removal(
        &self,
        insertion: &mut Insertion<'_, 'db>,
    ) -> Result<Option<usize>, Self::Error> {
        Ok(insertion.next_removal())
    }

    fn positive_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Positive(intersection.positive(self.db).iter()))
    }

    fn negative_elements(
        &self,
        intersection: IntersectionType<'db>,
    ) -> Result<Elements<'db>, Self::Error> {
        Ok(Elements::Negative(intersection.negative(self.db).iter()))
    }

    fn next_element(&self, elements: &mut Elements<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(match elements {
            Elements::Positive(elements) => elements.next().copied(),
            Elements::Negative(elements) => elements.next().copied(),
        })
    }

    fn empty_string(&self) -> Result<Type<'db>, Self::Error> {
        Ok(Type::string_literal(self.db, ""))
    }

    fn typeform_argument(&self, typeform: TypeFormType<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(typeform.type_argument(self.db))
    }

    fn resolve_alias(&self, ty: Type<'db>) -> Result<Type<'db>, Self::Error> {
        Ok(ty.resolve_type_alias(self.db))
    }

    fn subclass_from_instance(&self, ty: Type<'db>) -> Result<Option<Type<'db>>, Self::Error> {
        Ok(SubclassOfType::try_from_instance(self.db, self.env, ty).ok())
    }

    fn known_instance(&self, class: KnownClass) -> Result<Type<'db>, Self::Error> {
        Ok(class.to_instance(self.db, self.env))
    }

    fn has_known_class(
        &self,
        instance: NominalInstanceType<'db>,
        class: KnownClass,
    ) -> Result<bool, Self::Error> {
        Ok(instance.has_known_class(self.db, class))
    }

    fn known_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<Option<KnownClass>, Self::Error> {
        Ok(instance.known_class(self.db))
    }

    fn enum_class(&self, literal: EnumLiteralType<'db>) -> Result<ClassLiteral<'db>, Self::Error> {
        Ok(literal.enum_class(self.db))
    }

    fn instance_class(
        &self,
        instance: NominalInstanceType<'db>,
    ) -> Result<ClassLiteral<'db>, Self::Error> {
        Ok(instance.class_literal(self.db, self.env))
    }

    fn types_equal(&self, first: Type<'db>, second: Type<'db>) -> Result<bool, Self::Error> {
        Ok(first == second)
    }

    fn generic_intersection(
        &self,
        first: Type<'db>,
        second: Type<'db>,
    ) -> Result<Option<GenericIntersection<'db>>, Self::Error> {
        Ok(generic_gradual_intersection(
            self.db, self.env, first, second,
        ))
    }

    fn simplify_pair(
        &self,
        first: Type<'db>,
        second: Type<'db>,
        polarity: IntersectionPolarity,
    ) -> Result<IntersectionSimplification, Self::Error> {
        Ok(simplify_intersection_pair(
            self.db, self.env, first, second, polarity,
        ))
    }
}
