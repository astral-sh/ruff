def f():
    # These should both be left as-is
    I = 1  # noqa: ambiguous-variable-name, unused-variable


def f():
    # These should both be left as-is
    I = 1  # noqa: ambiguous-variable-name,unused-variable


def f():
    # This should be left as-is (including the comment)
    I = 1  # noqa: ambiguous-variable-name  comment


def f():
    # These should both be converted, but the comment should be left as-is (including spacing beforehand)
    I = 1  # noqa: E741 F841  comment


def f():
    # E741 should be converted to `ambiguous-variable-name`.
    I = 1  # noqa: E741, unused-variable  comment


def f():
    # `F841` should be converted to `unused-variable`.
    I = 1  # noqa: ambiguous-variable-name, F841


def f():
    # `F841` should be converted to `unused-variable`.
    I = 1  # noqa: ambiguous-variable-name, F841


def f():
    # These should both be converted.
    I = 1  # noqa: E741, F841
