# Typing-module aliases to other stdlib classes

The `typing` module has various aliases to other stdlib classes. These are a legacy feature, but
still need to be supported by a type checker.

## Correspondence

All of the following symbols can be mapped one-to-one with the actual type:

```py
import typing

def f(
    list_bare: typing.List,
    list_parametrized: typing.List[int],
    dict_bare: typing.Dict,
    dict_parametrized: typing.Dict[int, str],
    set_bare: typing.Set,
    set_parametrized: typing.Set[int],
    frozen_set_bare: typing.FrozenSet,
    frozen_set_parametrized: typing.FrozenSet[str],
    chain_map_bare: typing.ChainMap,
    chain_map_parametrized: typing.ChainMap[str, int],
    counter_bare: typing.Counter,
    counter_parametrized: typing.Counter[int],
    default_dict_bare: typing.DefaultDict,
    default_dict_parametrized: typing.DefaultDict[str, int],
    deque_bare: typing.Deque,
    deque_parametrized: typing.Deque[str],
    ordered_dict_bare: typing.OrderedDict,
    ordered_dict_parametrized: typing.OrderedDict[int, str],
):
    # TODO: revealed: list[Unknown]
    reveal_type(list_bare)  # revealed: list
    # TODO: revealed: list[int]
    reveal_type(list_parametrized)  # revealed: list

    reveal_type(dict_bare)  # revealed: dict[Unknown, Unknown]
    # TODO: revealed: dict[int, str]
    reveal_type(dict_parametrized)  # revealed: dict[Unknown, Unknown]

    # TODO: revealed: set[Unknown]
    reveal_type(set_bare)  # revealed: set
    # TODO: revealed: set[int]
    reveal_type(set_parametrized)  # revealed: set

    # TODO: revealed: frozenset[Unknown]
    reveal_type(frozen_set_bare)  # revealed: frozenset
    # TODO: revealed: frozenset[str]
    reveal_type(frozen_set_parametrized)  # revealed: frozenset

    reveal_type(chain_map_bare)  # revealed: ChainMap[Unknown, Unknown]
    # TODO: revealed: ChainMap[str, int]
    reveal_type(chain_map_parametrized)  # revealed: ChainMap[Unknown, Unknown]

    reveal_type(counter_bare)  # revealed: Counter[Unknown]
    # TODO: revealed: Counter[int]
    reveal_type(counter_parametrized)  # revealed: Counter[Unknown]

    reveal_type(default_dict_bare)  # revealed: defaultdict[Unknown, Unknown]
    # TODO: revealed: defaultdict[str, int]
    reveal_type(default_dict_parametrized)  # revealed: defaultdict[Unknown, Unknown]

    # TODO: revealed: deque[Unknown]
    reveal_type(deque_bare)  # revealed: deque
    # TODO: revealed: deque[str]
    reveal_type(deque_parametrized)  # revealed: deque

    reveal_type(ordered_dict_bare)  # revealed: OrderedDict[Unknown, Unknown]
    # TODO: revealed: OrderedDict[int, str]
    reveal_type(ordered_dict_parametrized)  # revealed: OrderedDict[Unknown, Unknown]
```

## Inheritance

The aliases can be inherited from. Some of these are still partially or wholly TODOs.

```py
import typing

####################
### Built-ins
####################

class ListSubclass(typing.List): ...

# TODO: generic protocols
# revealed: tuple[<class 'ListSubclass'>, <class 'list'>, <class 'MutableSequence'>, <class 'Sequence'>, <class 'Reversible'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, <class 'object'>]
reveal_type(ListSubclass.__mro__)

class DictSubclass(typing.Dict): ...

# TODO: generic protocols
# revealed: tuple[<class 'DictSubclass'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, typing.Generic[_KT, _VT_co], <class 'object'>]
reveal_type(DictSubclass.__mro__)

class SetSubclass(typing.Set): ...

# TODO: generic protocols
# revealed: tuple[<class 'SetSubclass'>, <class 'set'>, <class 'MutableSet'>, <class 'AbstractSet'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, <class 'object'>]
reveal_type(SetSubclass.__mro__)

class FrozenSetSubclass(typing.FrozenSet): ...

# TODO: generic protocols
# revealed: tuple[<class 'FrozenSetSubclass'>, <class 'frozenset'>, <class 'AbstractSet'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, <class 'object'>]
reveal_type(FrozenSetSubclass.__mro__)

####################
### `collections`
####################

class ChainMapSubclass(typing.ChainMap): ...

# TODO: generic protocols
# revealed: tuple[<class 'ChainMapSubclass'>, <class 'ChainMap[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, typing.Generic[_KT, _VT_co], <class 'object'>]
reveal_type(ChainMapSubclass.__mro__)

class CounterSubclass(typing.Counter): ...

# TODO: Should be (CounterSubclass, Counter, dict, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[<class 'CounterSubclass'>, <class 'Counter[Unknown]'>, <class 'dict[Unknown, int]'>, <class 'MutableMapping[Unknown, int]'>, <class 'Mapping[Unknown, int]'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, typing.Generic[_KT, _VT_co], typing.Generic[_T], <class 'object'>]
reveal_type(CounterSubclass.__mro__)

class DefaultDictSubclass(typing.DefaultDict): ...

# TODO: Should be (DefaultDictSubclass, defaultdict, dict, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[<class 'DefaultDictSubclass'>, <class 'defaultdict[Unknown, Unknown]'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, typing.Generic[_KT, _VT_co], <class 'object'>]
reveal_type(DefaultDictSubclass.__mro__)

class DequeSubclass(typing.Deque): ...

# TODO: generic protocols
# revealed: tuple[<class 'DequeSubclass'>, <class 'deque'>, <class 'MutableSequence'>, <class 'Sequence'>, <class 'Reversible'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, <class 'object'>]
reveal_type(DequeSubclass.__mro__)

class OrderedDictSubclass(typing.OrderedDict): ...

# TODO: Should be (OrderedDictSubclass, OrderedDict, dict, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[<class 'OrderedDictSubclass'>, <class 'OrderedDict[Unknown, Unknown]'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection'>, <class 'Iterable'>, <class 'Container'>, @Todo(`Protocol[]` subscript), typing.Generic, typing.Generic[_KT, _VT_co], <class 'object'>]
reveal_type(OrderedDictSubclass.__mro__)
```
