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
    # Neither of these are ignored and warning is
    # logged to user
    I = 1  # noqa: E741.F841
