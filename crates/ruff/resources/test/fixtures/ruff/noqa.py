def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741, F841


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741,F841


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741 F841


def f():
    # These should both be ignored by the `noqa`.
    I = 1  # noqa: E741 , F841


def f():
    # Only `E741` should be ignored by the `noqa`.
    I = 1  # noqa: E741.F841
