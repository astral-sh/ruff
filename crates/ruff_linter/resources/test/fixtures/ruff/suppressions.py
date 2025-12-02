def f():
    # These should both be ignored by the range suppression.
    # ruff: disable[E741, F841]
    I = 1
    # ruff: enable[E741, F841]


def f():
    # These should both be ignored by the range suppression.
    # ruff:disable[E741,F841]
    I = 1
    # ruff:enable[E741,F841]


def f():
    # These should both be ignored by the range suppression.
    # ruff: disable[E741]
    # ruff: disable[F841]
    I = 1
    # ruff: enable[E741]
    # ruff: enable[F841]


def f():
    # One should be ignored by the range suppression, and
    # the other logged to the user.
    # ruff: disable[E741]
    I = 1
    # ruff: enable[E741]


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
