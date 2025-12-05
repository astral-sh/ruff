def f():
    # These should both be ignored by the range suppression.
    # ruff: disable[E741, F841]
    I = 1
    # ruff: enable[E741, F841]


def f():
    # These should both be ignored by the implicit range suppression.
    # Should also generate an "unmatched suppression" warning.
    # ruff:disable[E741,F841]
    I = 1


def f():
    # Neither warning is ignored, and an "unmatched suppression"
    # should be generated.
    I = 1
    # ruff: enable[E741, F841]


def f():
    # One should be ignored by the range suppression, and
    # the other logged to the user.
    # ruff: disable[E741]
    I = 1
    # ruff: enable[E741]


def f():
    # Test interleaved range suppressions. The first and last
    # lines should each log a different warning, while the
    # middle line should be completely silenced.
    # ruff: disable[E741]
    l = 0
    # ruff: disable[F841]
    O = 1
    # ruff: enable[E741]
    I = 2
    # ruff: enable[F841]


def f():
    # Neither of these are ignored and warnings are
    # logged to user
    # ruff: disable[E501]
    I = 1
    # ruff: enable[E501]


def f():
    # These should both be ignored by the range suppression,
    # and an unusued noqa diagnostic should be logged.
    # ruff:disable[E741,F841]
    I = 1  # noqa: E741,F841
    # ruff:enable[E741,F841]
