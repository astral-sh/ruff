"""Test case expected to be run with `suppress_dummy_args = True`."""

# OK
def foo(_) -> None:
    ...


# OK
def foo(*_) -> None:
    ...


# OK
def foo(**_) -> None:
    ...


# OK
def foo(a: int, _) -> None:
    ...


# OK
def foo() -> None:
    def bar(_) -> None:
        ...
