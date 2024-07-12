def f():
    # These should both be left as-is
    I = 1  # noqa: E741, F841


def f():
    # These should both be left as-is
    I = 1  # noqa: E741,F841


def f():
    # This should be left as-is (including the comment)
    I = 1  # noqa: E741  comment


def f():
    # These should both be converted, but the comment should be left as-is (including spacing beforehand)
    I = 1  # noqa: ambiguous-variable-name,unused-variable  comment


def f():
    # `ambiguous-variable-name` should be converted to `E741`.
    I = 1  # noqa: ambiguous-variable-name F841  comment


def f():
    # `ambiguous-variable-name` should be converted to `E741`.
    I = 1  # noqa: ambiguous-variable-name, F841


def f():
    # `unused-variable` should be converted to `F841`.
    I = 1  # noqa: E741, unused-variable


def f():
    # These should both be converted.
    I = 1  # noqa: ambiguous-variable-name, unused-variable
