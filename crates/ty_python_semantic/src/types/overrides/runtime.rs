use ruff_python_ast::name::Name;
use salsa::execution_probe::{NativeValueQuote, PassiveMemoProfile, QueryKeyProfile};
use salsa::plumbing::function::{Configuration, InternedQueryConfiguration};
use salsa::plumbing::{QuoteError, QuoteFuel};
use ty_python_core::scope::ScopeId;
use ty_python_core::symbol::ScopedSymbolId;

use super::VariableKind;
use crate::Db;
use crate::types::ClassType;

pub(in crate::types) trait EffectiveVariableKindConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (ClassType<'a>, Name)>
    + for<'a> Configuration<DbView = dyn Db, Output<'a> = Option<VariableKind>>
{
}

impl<C> EffectiveVariableKindConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (ClassType<'a>, Name)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = Option<VariableKind>>
{
}

pub(in crate::types) struct EffectiveVariableKindProfile;

impl EffectiveVariableKindProfile {
    fn input_work_for_name_length(name_bytes: usize) -> Option<usize> {
        // ClassType hashes and compares scalar variants and interned handles. Name can
        // inspect its string bytes; dropping its shared representation does not traverse them.
        4usize.checked_add(name_bytes)
    }

    pub(in crate::types) fn input_conversion_quote() -> NativeValueQuote {
        // The generated tuple clone copies the class and shallow-clones Name. A heap-backed
        // Name shares its buffer; cleanup releases that reference without visiting characters.
        NativeValueQuote {
            work: 3,
            requested_bytes: size_of::<(ClassType<'_>, Name)>(),
            cleanup_work: 1,
        }
    }

    pub(in crate::types) fn output_comparison_quote() -> NativeValueQuote {
        NativeValueQuote {
            work: 2,
            requested_bytes: size_of::<bool>(),
            cleanup_work: 0,
        }
    }
}

impl<C: EffectiveVariableKindConfiguration> QueryKeyProfile<C> for EffectiveVariableKindProfile {
    fn input_work<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        Self::input_work_for_name_length(input.1.len())
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

impl<C: EffectiveVariableKindConfiguration> PassiveMemoProfile<C> for EffectiveVariableKindProfile {
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        Some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

pub(in crate::types) trait FunctionDefinitionConfiguration:
    InternedQueryConfiguration
    + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (ScopeId<'a>, ScopedSymbolId)>
    + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>
{
}

impl<C> FunctionDefinitionConfiguration for C where
    C: InternedQueryConfiguration
        + for<'a> salsa::plumbing::interned::Configuration<Fields<'a> = (ScopeId<'a>, ScopedSymbolId)>
        + for<'a> Configuration<DbView = dyn Db, Output<'a> = bool>
{
}

pub(in crate::types) struct FunctionDefinitionProfile;

impl FunctionDefinitionProfile {
    pub(in crate::types) fn input_conversion_quote() -> NativeValueQuote {
        NativeValueQuote {
            work: 3,
            requested_bytes: size_of::<(ScopeId<'_>, ScopedSymbolId)>(),
            cleanup_work: 0,
        }
    }

    pub(in crate::types) fn output_comparison_quote() -> NativeValueQuote {
        NativeValueQuote {
            work: 1,
            requested_bytes: size_of::<bool>(),
            cleanup_work: 0,
        }
    }
}

impl<C: FunctionDefinitionConfiguration> QueryKeyProfile<C> for FunctionDefinitionProfile {
    fn input_work<'db>(
        _input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
    ) -> Option<usize> {
        Some(2)
    }

    fn input_work_bounded<'db>(
        input: &<C as salsa::plumbing::interned::Configuration>::Fields<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as QueryKeyProfile<C>>::input_work(input).ok_or(QuoteError::Overflow)
    }
}

impl<C: FunctionDefinitionConfiguration> PassiveMemoProfile<C> for FunctionDefinitionProfile {
    fn retired_output_work<'db>(_output: &C::Output<'db>) -> Option<usize> {
        Some(0)
    }

    fn retired_output_work_bounded<'db>(
        output: &C::Output<'db>,
        fuel: &mut QuoteFuel,
    ) -> Result<usize, QuoteError> {
        fuel.consume(1)?;
        <Self as PassiveMemoProfile<C>>::retired_output_work(output).ok_or(QuoteError::Overflow)
    }
}

#[cfg(test)]
mod tests;
