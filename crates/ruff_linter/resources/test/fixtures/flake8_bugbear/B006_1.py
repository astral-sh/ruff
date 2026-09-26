# Docstring followed by a newline

def foobar(foor, bar={}):    
    """
    """
# Tests for https://github.com/astral-sh/ruff/issues/28735: same-file type aliases
# in annotations.
from typing import Mapping, TypeAlias

CustomMapping = Mapping[str, str]
ChainedMapping = CustomMapping
MutableAlias = dict[str, str]
Pep613Alias: TypeAlias = Mapping[str, str]


# OK: the annotation is a same-file alias of an immutable generic.
def ok_alias(d: CustomMapping = {}):
    ...


# OK: chained same-file aliases resolve to an immutable generic.
def ok_chained_alias(d: ChainedMapping = {}):
    ...


# OK: PEP 613 alias of an immutable generic.
def ok_pep613_alias(d: Pep613Alias = {}):
    ...


# Error: the alias resolves to a mutable generic.
def err_mutable_alias(d: MutableAlias = {}):
    ...


SelfRef = SelfRef


# Error: the alias is self-referential and cannot be resolved.
def err_self_ref(d: SelfRef = {}):
    ...


# Error: the alias is defined after the function, so it can't be resolved yet.
def err_forward_alias(d: ForwardAlias = {}):
    ...


ForwardAlias = Mapping[str, str]
