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
    reveal_type(list_bare)  # revealed: list[Unknown]
    # TODO: revealed: list[int]
    reveal_type(list_parametrized)  # revealed: list[Unknown]

    reveal_type(dict_bare)  # revealed: dict[Unknown, Unknown]
    # TODO: revealed: dict[int, str]
    reveal_type(dict_parametrized)  # revealed: dict[Unknown, Unknown]

    reveal_type(set_bare)  # revealed: set[Unknown]
    # TODO: revealed: set[int]
    reveal_type(set_parametrized)  # revealed: set[Unknown]

    # TODO: revealed: frozenset[Unknown]
    reveal_type(frozen_set_bare)  # revealed: frozenset[Unknown]
    # TODO: revealed: frozenset[str]
    reveal_type(frozen_set_parametrized)  # revealed: frozenset[Unknown]

    reveal_type(chain_map_bare)  # revealed: ChainMap[Unknown, Unknown]
    # TODO: revealed: ChainMap[str, int]
    reveal_type(chain_map_parametrized)  # revealed: ChainMap[Unknown, Unknown]

    reveal_type(counter_bare)  # revealed: Counter[Unknown]
    # TODO: revealed: Counter[int]
    reveal_type(counter_parametrized)  # revealed: Counter[Unknown]

    reveal_type(default_dict_bare)  # revealed: defaultdict[Unknown, Unknown]
    # TODO: revealed: defaultdict[str, int]
    reveal_type(default_dict_parametrized)  # revealed: defaultdict[Unknown, Unknown]

    reveal_type(deque_bare)  # revealed: deque[Unknown]
    # TODO: revealed: deque[str]
    reveal_type(deque_parametrized)  # revealed: deque[Unknown]

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

# revealed: tuple[<class 'ListSubclass'>, <class 'list[Unknown]'>, <class 'MutableSequence[Unknown]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(ListSubclass.__mro__)

class DictSubclass(typing.Dict): ...

# revealed: tuple[<class 'DictSubclass'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(DictSubclass.__mro__)

class SetSubclass(typing.Set): ...

# revealed: tuple[<class 'SetSubclass'>, <class 'set[Unknown]'>, <class 'MutableSet[Unknown]'>, <class 'AbstractSet[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(SetSubclass.__mro__)

class FrozenSetSubclass(typing.FrozenSet): ...

# revealed: tuple[<class 'FrozenSetSubclass'>, <class 'frozenset[Unknown]'>, <class 'AbstractSet[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(FrozenSetSubclass.__mro__)

####################
### `collections`
####################

class ChainMapSubclass(typing.ChainMap): ...

# revealed: tuple[<class 'ChainMapSubclass'>, <class 'ChainMap[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(ChainMapSubclass.__mro__)

class CounterSubclass(typing.Counter): ...

# revealed: tuple[<class 'CounterSubclass'>, <class 'Counter[Unknown]'>, <class 'dict[Unknown, int]'>, <class 'MutableMapping[Unknown, int]'>, <class 'Mapping[Unknown, int]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(CounterSubclass.__mro__)

class DefaultDictSubclass(typing.DefaultDict): ...

# revealed: tuple[<class 'DefaultDictSubclass'>, <class 'defaultdict[Unknown, Unknown]'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(DefaultDictSubclass.__mro__)

class DequeSubclass(typing.Deque): ...

# revealed: tuple[<class 'DequeSubclass'>, <class 'deque[Unknown]'>, <class 'MutableSequence[Unknown]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(DequeSubclass.__mro__)

class OrderedDictSubclass(typing.OrderedDict): ...

# revealed: tuple[<class 'OrderedDictSubclass'>, <class 'OrderedDict[Unknown, Unknown]'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>]
reveal_type(OrderedDictSubclass.__mro__)
```
