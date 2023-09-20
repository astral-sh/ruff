import collections
import datetime as dt
from decimal import Decimal
from fractions import Fraction
import logging
import operator
from pathlib import Path
import random
import re
import time
import types
from operator import attrgetter, itemgetter, methodcaller
from types import MappingProxyType


# B006
# Allow immutable literals/calls/comprehensions
def this_is_okay(value=(1, 2, 3)):
    ...


async def and_this_also(value=tuple()):
    pass


def frozenset_also_okay(value=frozenset()):
    pass


def mappingproxytype_okay(
    value=MappingProxyType({}), value2=types.MappingProxyType({})
):
    pass


def re_compile_ok(value=re.compile("foo")):
    pass


def operators_ok(
    v=operator.attrgetter("foo"),
    v2=operator.itemgetter("foo"),
    v3=operator.methodcaller("foo"),
):
    pass


def operators_ok_unqualified(
    v=attrgetter("foo"),
    v2=itemgetter("foo"),
    v3=methodcaller("foo"),
):
    pass


def kwonlyargs_immutable(*, value=()):
    ...


# Flag mutable literals/comprehensions


def this_is_wrong(value=[1, 2, 3]):
    ...


def this_is_also_wrong(value={}):
    ...


class Foo:
    @staticmethod
    def this_is_also_wrong_and_more_indented(value={}):
        pass


def multiline_arg_wrong(value={

}):
    ...

def single_line_func_wrong(value = {}): ...


def and_this(value=set()):
    ...


def this_too(value=collections.OrderedDict()):
    ...


async def async_this_too(value=collections.defaultdict()):
    ...


def dont_forget_me(value=collections.deque()):
    ...


# N.B. we're also flagging the function call in the comprehension
def list_comprehension_also_not_okay(default=[i**2 for i in range(3)]):
    pass


def dict_comprehension_also_not_okay(default={i: i**2 for i in range(3)}):
    pass


def set_comprehension_also_not_okay(default={i**2 for i in range(3)}):
    pass


def kwonlyargs_mutable(*, value=[]):
    ...


# Recommended approach for mutable defaults
def do_this_instead(value=None):
    if value is None:
        value = set()


# B008
# Flag function calls as default args (including if they are part of a sub-expression)
def in_fact_all_calls_are_wrong(value=time.time()):
    ...


def f(when=dt.datetime.now() + dt.timedelta(days=7)):
    pass


def can_even_catch_lambdas(a=(lambda x: x)()):
    ...


# Recommended approach for function calls as default args
LOGGER = logging.getLogger(__name__)


def do_this_instead_of_calls_in_defaults(logger=LOGGER):
    # That makes it more obvious that this one value is reused.
    ...


# Handle inf/infinity/nan special case
def float_inf_okay(value=float("inf")):
    pass


def float_infinity_okay(value=float("infinity")):
    pass


def float_plus_infinity_okay(value=float("+infinity")):
    pass


def float_minus_inf_okay(value=float("-inf")):
    pass


def float_nan_okay(value=float("nan")):
    pass


def float_minus_NaN_okay(value=float("-NaN")):
    pass


def float_infinity_literal(value=float("1e999")):
    pass


# Allow standard floats
def float_int_okay(value=float(3)):
    pass


def float_str_not_inf_or_nan_okay(value=float("3.14")):
    pass


# Allow immutable str() value
def str_okay(value=str("foo")):
    pass


# Allow immutable bool() value
def bool_okay(value=bool("bar")):
    pass

# Allow immutable bytes() value
def bytes_okay(value=bytes(1)):
    pass

# Allow immutable int() value
def int_okay(value=int("12")):
    pass


# Allow immutable complex() value
def complex_okay(value=complex(1,2)):
    pass


# Allow immutable Fraction() value
def fraction_okay(value=Fraction(1,2)):
    pass


# Allow decimals
def decimal_okay(value=Decimal("0.1")):
    pass

# Allow dates
def date_okay(value=dt.date(2023, 3, 27)):
    pass

# Allow datetimes
def datetime_okay(value=dt.datetime(2023, 3, 27, 13, 51, 59)):
    pass

# Allow timedeltas
def timedelta_okay(value=dt.timedelta(hours=1)):
    pass

# Allow paths
def path_okay(value=Path(".")):
    pass

# B008 allow arbitrary call with immutable annotation
def immutable_annotation_call(value: Sequence[int] = foo()):
    pass

# B006 and B008
# We should handle arbitrary nesting of these B008.
def nested_combo(a=[float(3), dt.datetime.now()]):
    pass


# Don't flag nested B006 since we can't guarantee that
# it isn't made mutable by the outer operation.
def no_nested_b006(a=map(lambda s: s.upper(), ["a", "b", "c"])):
    pass


# B008-ception.
def nested_b008(a=random.randint(0, dt.datetime.now().year)):
    pass


# Ignore lambda contents since they are evaluated at call time.
def foo(f=lambda x: print(x)):
    f(1)


from collections import abc
from typing import Annotated, Dict, Optional, Sequence, Union, Set
import typing_extensions


def immutable_annotations(
    a: Sequence[int] | None = [],
    b: Optional[abc.Mapping[int, int]] = {},
    c: Annotated[Union[abc.Set[str], abc.Sized], "annotation"] = set(),
    d: typing_extensions.Annotated[
        Union[abc.Set[str], abc.Sized], "annotation"
    ] = set(),
):
    pass


def mutable_annotations(
    a: list[int] | None = [],
    b: Optional[Dict[int, int]] = {},
    c: Annotated[Union[Set[str], abc.Sized], "annotation"] = set(),
    d: typing_extensions.Annotated[Union[Set[str], abc.Sized], "annotation"] = set(),
):
    pass


def single_line_func_wrong(value: dict[str, str] = {}):
    """Docstring"""


def single_line_func_wrong(value: dict[str, str] = {}):
    """Docstring"""
    ...


def single_line_func_wrong(value: dict[str, str] = {}):
    """Docstring"""; ...


def single_line_func_wrong(value: dict[str, str] = {}):
    """Docstring"""; \
        ...


def single_line_func_wrong(value: dict[str, str] = {
    # This is a comment
}):
    """Docstring"""


def single_line_func_wrong(value: dict[str, str] = {}) \
    : \
    """Docstring"""


def single_line_func_wrong(value: dict[str, str] = {}):
    """Docstring without newline"""