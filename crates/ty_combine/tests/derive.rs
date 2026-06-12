use ty_combine::Combine;

#[derive(Debug, PartialEq, ruff_macros::Combine)]
struct Generic<T>
where
    T: Combine,
{
    value: T,
}

#[test]
fn generic() {
    assert_eq!(
        Generic { value: Some(1) }.combine(Generic { value: Some(2) }),
        Generic { value: Some(1) }
    );
}
