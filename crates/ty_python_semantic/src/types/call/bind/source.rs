//! Storage for bindings built from completed function or synthetic signatures.

use super::ownership::pristine_bindings_retirement_work;
use super::{Binding, Bindings};
use crate::types::signatures::{CallableSignature, Signature};

pub(in crate::types) struct InitialBindingsQuote {
    pub work: usize,
    pub bytes: usize,
}

pub(in crate::types) fn initial_bindings_quote(
    signature: &CallableSignature<'_>,
) -> Option<InitialBindingsQuote> {
    initial_overloads_quote(&signature.overloads)
}

pub(in crate::types) fn initial_overloads_quote(
    signatures: &[Signature<'_>],
) -> Option<InitialBindingsQuote> {
    let count = signatures.len();
    // Fixed initialization costs scalar work and representation bytes, including inline values.
    let mut work = 1usize
        .checked_add(count)?
        .checked_add(pristine_bindings_retirement_work(count)?)?;
    let fixed_bytes =
        size_of::<Bindings<'_>>().checked_add(count.checked_mul(size_of::<Binding<'_>>())?)?;
    // The constructor collects into SmallVec<[Binding; 1]>. Include a conservative growth
    // bound for a spilled vector; its remaining fields start empty or contain scalar handles.
    let spill_bytes = if count <= 1 {
        0
    } else {
        count
            .checked_next_power_of_two()?
            .checked_mul(2)?
            .checked_mul(size_of::<Binding<'_>>())?
    };
    let mut bytes = fixed_bytes.checked_add(spill_bytes)?;
    for overload in signatures {
        work = work
            .checked_add(16)?
            .checked_add(overload.retirement_work()?)?;
        bytes = bytes.checked_add(overload.clone_requested_bytes()?)?;
    }
    Some(InitialBindingsQuote { work, bytes })
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::{initial_bindings_quote, initial_overloads_quote};
    use crate::types::Type;
    use crate::types::call::bind::{Binding, Bindings, CallableBinding};
    use crate::types::signatures::{CallableSignature, Parameter, Parameters, Signature};

    #[test]
    fn fixed_binding_representation_uses_bytes() -> anyhow::Result<()> {
        // Initial owners fund the fixed iterative destruction scans separately from their bytes.
        let empty = initial_overloads_quote(&[]).context("empty bindings quote")?;
        assert_eq!(empty.work, 49);
        assert_eq!(empty.bytes, size_of::<Bindings<'_>>());

        let signature =
            CallableSignature::single(Signature::new(Parameters::empty(), Type::unknown()));
        let quote = initial_bindings_quote(&signature).context("single binding quote")?;
        let bindings = Bindings::from(CallableBinding::from_signature(Type::unknown(), &signature));
        assert_eq!(quote.work, 90);
        assert!(quote.bytes >= size_of_val(&bindings) + size_of::<Binding<'_>>());
        Ok(())
    }

    #[test]
    fn initial_bindings_preserve_signature_cleanup_and_spill_storage() -> anyhow::Result<()> {
        // Parameter/extras retirement and spilled overload storage remain funded by initialization.
        let empty = Signature::new(Parameters::empty(), Type::unknown());
        let parameters = Signature::new(
            Parameters::standard([
                Parameter::positional_only(None),
                Parameter::positional_only(None),
            ]),
            Type::unknown(),
        );
        let extras = parameters.clone().with_source_overload_index(Some(0));
        let quote = |signature| {
            initial_overloads_quote(std::slice::from_ref(signature)).context("single binding quote")
        };
        let empty_quote = quote(&empty)?;
        let parameters_quote = quote(&parameters)?;
        let extras_quote = quote(&extras)?;
        assert_eq!(parameters_quote.work - empty_quote.work, 16);
        assert_eq!(parameters_quote.bytes, empty_quote.bytes);
        assert_eq!(extras_quote.work - parameters_quote.work, 3);
        assert!(extras_quote.bytes > parameters_quote.bytes);

        let signatures = [empty, parameters, extras];
        let quote = initial_overloads_quote(&signatures).context("spilled binding quote")?;
        let callable = CallableBinding::from_overloads(Type::unknown(), signatures);
        assert!(callable.overloads.spilled());
        assert!(
            quote.bytes
                >= size_of::<Bindings<'_>>()
                    + (callable.overloads.len() + callable.overloads.capacity())
                        * size_of::<Binding<'_>>()
        );
        Ok(())
    }
}
