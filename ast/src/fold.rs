use super::generic::*;

use crate::{builtin, ConversionFlag};

pub trait Foldable<T, U> {
    type Mapped;
    fn fold<F: Fold<T, TargetU = U> + ?Sized>(
        self,
        folder: &mut F,
    ) -> Result<Self::Mapped, F::Error>;
}

impl<T, U, X> Foldable<T, U> for Vec<X>
where
    X: Foldable<T, U>,
{
    type Mapped = Vec<X::Mapped>;
    fn fold<F: Fold<T, TargetU = U> + ?Sized>(
        self,
        folder: &mut F,
    ) -> Result<Self::Mapped, F::Error> {
        self.into_iter().map(|x| x.fold(folder)).collect()
    }
}

impl<T, U, X> Foldable<T, U> for Option<X>
where
    X: Foldable<T, U>,
{
    type Mapped = Option<X::Mapped>;
    fn fold<F: Fold<T, TargetU = U> + ?Sized>(
        self,
        folder: &mut F,
    ) -> Result<Self::Mapped, F::Error> {
        self.map(|x| x.fold(folder)).transpose()
    }
}

impl<T, U, X> Foldable<T, U> for Box<X>
where
    X: Foldable<T, U>,
{
    type Mapped = Box<X::Mapped>;
    fn fold<F: Fold<T, TargetU = U> + ?Sized>(
        self,
        folder: &mut F,
    ) -> Result<Self::Mapped, F::Error> {
        (*self).fold(folder).map(Box::new)
    }
}

macro_rules! simple_fold {
    ($($t:ty),+$(,)?) => {
        $(impl<T, U> $crate::fold::Foldable<T, U> for $t {
            type Mapped = Self;
            #[inline]
            fn fold<F: Fold<T, TargetU = U> + ?Sized>(
                self,
                _folder: &mut F,
            ) -> Result<Self::Mapped, F::Error> {
                Ok(self)
            }
        })+
    };
}

simple_fold!(
    builtin::Int,
    builtin::String,
    builtin::Identifier,
    bool,
    ConversionFlag,
    builtin::Constant
);

include!("gen/fold.rs");
