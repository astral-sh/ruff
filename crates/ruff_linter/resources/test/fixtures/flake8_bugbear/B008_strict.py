"""Fixture for the `strict-argument-defaults` option (B008).

With `strict-argument-defaults = true`, function calls in argument
defaults are flagged even when the parameter's annotation is an immutable type.
Immutable calls and `NewType` calls over immutable types remain exempt.
"""

import random
from typing import NewType, Sequence


def get_current_username() -> str:
    return "someone"


def foo() -> Sequence[int]:
    return [1, 2, 3]


# Errors: annotation is immutable, but the default is a (non-immutable) call.
# These are only flagged when `strict-argument-defaults` is enabled.
def get_random_number(num: int = random.randint(1, 100)) -> int:
    return num


def log_message(message: str, username: str = get_current_username()):
    ...


def immutable_annotation_call(value: Sequence[int] = foo()):
    ...


# OK: not a call.
def no_call(value: int = 5):
    ...


# OK: immutable call is exempt regardless of this option.
def immutable_call(value: tuple = tuple()):
    ...


def float_inf_okay(value: float = float("inf")):
    ...


# OK: `NewType` over an immutable type is exempt regardless of this option.
Number = NewType("Number", int)


def newtype_okay(value: int = Number(5)):
    ...
