def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name, unused-variable


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name,unused-variable


def f():
    # Only `ambiguous-variable-name` should be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name unused-variable


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name , unused-variable


def f():
    # Neither of these should be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name.unused-variable
