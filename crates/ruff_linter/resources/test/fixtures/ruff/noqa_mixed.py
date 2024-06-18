def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741, unused-variable


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name, F841


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741,unused-variable


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name,F841


def f():
    # Only `E741` should be ignored by the `noqa`.
    I = 1  # noqa: E741 unused-variable


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name F841



def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741 , unused-variable


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name , F841


def f():
    # Only `E741` should be ignored by the `noqa`.
    I = 1  # noqa: E741.unused-variable


def f():
    # Neither of these should be ignored by the `noqa`.
    I = 1  # noqa: ambiguous-variable-name.unused-variable
