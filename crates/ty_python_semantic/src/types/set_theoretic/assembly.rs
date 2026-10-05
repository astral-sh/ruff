//! Lazy element traversal shared by ordinary and controlled union/intersection assembly.

use std::convert::Infallible;

use super::{IntersectionBuilder, UnionBuilder};
use crate::types::Type;
use crate::{Db, ProgramEnvironment};

pub(in crate::types) trait TypeElements<'db> {
    type Error;
    type Item: Into<Type<'db>>;

    async fn next(&mut self) -> Result<Option<Self::Item>, Self::Error>;
}

pub(in crate::types) trait TypeAssemblyEffects<'db> {
    type Error;

    async fn union<I: TypeElements<'db, Error = Self::Error>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> Result<Type<'db>, Self::Error>;

    async fn intersection<I: TypeElements<'db, Error = Self::Error>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> Result<Type<'db>, Self::Error>;
}

pub(super) struct InlineTypeElements<I>(pub(super) I);

impl<'db, I, T> TypeElements<'db> for InlineTypeElements<I>
where
    I: Iterator<Item = T>,
    T: Into<Type<'db>>,
{
    type Error = Infallible;
    type Item = T;

    async fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self.0.next())
    }
}

pub(in crate::types) struct InlineTypeAssembly;

impl<'db> TypeAssemblyEffects<'db> for InlineTypeAssembly {
    type Error = Infallible;

    async fn union<I: TypeElements<'db, Error = Infallible>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> Result<Type<'db>, Infallible> {
        let mut builder = UnionBuilder::new(db, env);
        builder.add_in_place(first.into());
        builder.add_in_place(second.into());
        while let Some(element) = remaining.next().await? {
            builder.add_in_place(element.into());
        }
        Ok(builder.build())
    }

    async fn intersection<I: TypeElements<'db, Error = Infallible>>(
        &self,
        db: &'db dyn Db,
        env: &ProgramEnvironment<'db>,
        first: I::Item,
        second: I::Item,
        remaining: &mut I,
    ) -> Result<Type<'db>, Infallible> {
        let mut builder =
            IntersectionBuilder::new(db, env).positive_elements([first.into(), second.into()]);
        while let Some(element) = remaining.next().await? {
            builder.add_positive_in_place(element.into());
        }
        Ok(builder.build())
    }
}

pub(in crate::types) async fn union_from_elements<'db, E, I>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    elements: &mut I,
    effects: &E,
) -> Result<Type<'db>, E::Error>
where
    E: TypeAssemblyEffects<'db>,
    I: TypeElements<'db, Error = E::Error>,
{
    if let Some(first) = elements.next().await? {
        if let Some(second) = elements.next().await? {
            effects.union(db, env, first, second, elements).await
        } else {
            Ok(first.into())
        }
    } else {
        Ok(Type::Never)
    }
}

pub(in crate::types) async fn intersection_from_elements<'db, E, I>(
    db: &'db dyn Db,
    env: &ProgramEnvironment<'db>,
    elements: &mut I,
    effects: &E,
) -> Result<Type<'db>, E::Error>
where
    E: TypeAssemblyEffects<'db>,
    I: TypeElements<'db, Error = E::Error>,
{
    if let Some(first) = elements.next().await? {
        if let Some(second) = elements.next().await? {
            effects.intersection(db, env, first, second, elements).await
        } else {
            Ok(first.into())
        }
    } else {
        Ok(Type::object())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;
    use crate::db::tests::setup_db;
    use crate::types::{IntersectionType, UnionType};

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Next(usize),
        Convert(usize),
    }

    struct Element<'a> {
        index: usize,
        events: &'a RefCell<Vec<Event>>,
    }

    impl<'db> From<Element<'_>> for Type<'db> {
        fn from(element: Element<'_>) -> Self {
            element
                .events
                .borrow_mut()
                .push(Event::Convert(element.index));
            Type::unknown()
        }
    }

    #[test]
    fn ordinary_element_traversal_order() {
        let db = setup_db();
        let env = db.program_environment();
        for intersection in [false, true] {
            for count in [0, 1, 3] {
                let events = RefCell::new(Vec::new());
                let mut index = 0;
                let elements = std::iter::from_fn(|| {
                    events.borrow_mut().push(Event::Next(index));
                    if index < count {
                        let element = Element {
                            index,
                            events: &events,
                        };
                        index += 1;
                        Some(element)
                    } else {
                        None
                    }
                });
                let actual = if intersection {
                    IntersectionType::from_elements(&db, &env, elements)
                } else {
                    UnionType::from_elements(&db, &env, elements)
                };
                let expected_type = match (intersection, count) {
                    (false, 0) => Type::Never,
                    (true, 0) => Type::object(),
                    _ => Type::unknown(),
                };
                assert_eq!(actual, expected_type);
                let expected_events = match count {
                    0 => vec![Event::Next(0)],
                    1 => vec![Event::Next(0), Event::Next(1), Event::Convert(0)],
                    _ => vec![
                        Event::Next(0),
                        Event::Next(1),
                        Event::Convert(0),
                        Event::Convert(1),
                        Event::Next(2),
                        Event::Convert(2),
                        Event::Next(3),
                    ],
                };
                assert_eq!(*events.borrow(), expected_events);
            }
        }
    }
}
