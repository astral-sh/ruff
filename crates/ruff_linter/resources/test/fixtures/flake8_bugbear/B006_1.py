# Docstring followed by a newline

def foobar(foor, bar={}):    
    """
    """


from typing import Mapping

CustomMapping = Mapping[str, str]

ChainedMapping = CustomMapping

CustomDict = dict[str, str]


# OK: the annotation aliases an immutable generic type.
def immutable_alias(d: CustomMapping = {}):
    ...


# OK: chained aliases resolve to an immutable generic type.
def chained_immutable_alias(d: ChainedMapping = {}):
    ...


# Error: the alias resolves to a mutable type.
def mutable_alias(d: CustomDict = {}):
    ...

SelfRef = SelfRef


# Error: the alias is self-referential and cannot be resolved.
def self_referential_alias(d: SelfRef = {}):
    ...
