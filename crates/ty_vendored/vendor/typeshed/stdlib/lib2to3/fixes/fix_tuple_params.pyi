"""Fixer for function definitions with tuple parameters.

def func(((a, b), c), d):
    ...

    ->

def func(x, d):
    ((a, b), c) = x
    ...

It will also support lambdas:

    lambda (x, y): x + y -> lambda t: t[0] + t[1]

    # The parens are a syntax error in Python 3
    lambda (x): x + y -> lambda x: x + y
"""

from typing import ClassVar, Literal

from .. import fixer_base

def is_docstring(stmt): ...

class FixTupleParams(fixer_base.BaseFix):
    BM_compatible: ClassVar[Literal[True]]
    PATTERN: ClassVar[str]
    def transform(self, node, results): ...
    def transform_lambda(self, node, results) -> None: ...

def simplify_args(node): ...
def find_params(node): ...
def map_to_index(param_list, prefix=[], d=None): ...
def tuple_name(param_list): ...
