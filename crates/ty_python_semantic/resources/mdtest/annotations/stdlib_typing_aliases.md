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
    reveal_type(list_parametrized)  # revealed: list[int]

    reveal_type(dict_bare)  # revealed: dict[Unknown, Unknown]
    reveal_type(dict_parametrized)  # revealed: dict[int, str]

    reveal_type(set_bare)  # revealed: set[Unknown]
    reveal_type(set_parametrized)  # revealed: set[int]

    reveal_type(frozen_set_bare)  # revealed: frozenset[Unknown]
    reveal_type(frozen_set_parametrized)  # revealed: frozenset[str]

    reveal_type(chain_map_bare)  # revealed: ChainMap[Unknown, Unknown]
    reveal_type(chain_map_parametrized)  # revealed: ChainMap[str, int]

    reveal_type(counter_bare)  # revealed: Counter[Unknown]
    reveal_type(counter_parametrized)  # revealed: Counter[int]

    reveal_type(default_dict_bare)  # revealed: defaultdict[Unknown, Unknown]
    reveal_type(default_dict_parametrized)  # revealed: defaultdict[str, int]

    reveal_type(deque_bare)  # revealed: deque[Unknown]
    reveal_type(deque_parametrized)  # revealed: deque[str]

    reveal_type(ordered_dict_bare)  # revealed: OrderedDict[Unknown, Unknown]
    reveal_type(ordered_dict_parametrized)  # revealed: OrderedDict[int, str]
```

## Incorrect number of type arguments

In case the incorrect number of type arguments is passed, a diagnostic is given.

```py
import typing

def f(
    # error: [invalid-type-form] "Legacy alias `typing.List` expected exactly 1 argument, got 2"
    incorrect_list: typing.List[int, int],
    # error: [invalid-type-form] "Legacy alias `typing.Dict` expected exactly 2 arguments, got 3"
    incorrect_dict: typing.Dict[int, int, int],
    # error: [invalid-type-form] "Legacy alias `typing.Dict` expected exactly 2 arguments, got 1"
    incorrect_dict2: typing.Dict[int],  # type argument is not a tuple here
    # error: [invalid-type-form]
    incorrect_set: typing.Set[int, int],
    # error: [invalid-type-form]
    incorrect_frozen_set: typing.FrozenSet[int, int],
    # error: [invalid-type-form]
    incorrect_chain_map: typing.ChainMap[int, int, int],
    # error: [invalid-type-form]
    incorrect_chain_map2: typing.ChainMap[int],
    # error: [invalid-type-form]
    incorrect_counter: typing.Counter[int, int],
    # error: [invalid-type-form]
    incorrect_default_dict: typing.DefaultDict[int, int, int],
    # error: [invalid-type-form]
    incorrect_default_dict2: typing.DefaultDict[int],
    # error: [invalid-type-form]
    incorrect_deque: typing.Deque[int, int],
    # error: [invalid-type-form]
    incorrect_ordered_dict: typing.OrderedDict[int, int, int],
    # error: [invalid-type-form]
    incorrect_ordered_dict2: typing.OrderedDict[int],
):
    reveal_type(incorrect_list)  # revealed: list[Unknown]
    reveal_type(incorrect_dict)  # revealed: dict[Unknown, Unknown]
    reveal_type(incorrect_dict2)  # revealed: dict[Unknown, Unknown]
    reveal_type(incorrect_set)  # revealed: set[Unknown]
    reveal_type(incorrect_frozen_set)  # revealed: frozenset[Unknown]
    reveal_type(incorrect_chain_map)  # revealed: ChainMap[Unknown, Unknown]
    reveal_type(incorrect_chain_map2)  # revealed: ChainMap[Unknown, Unknown]
    reveal_type(incorrect_counter)  # revealed: Counter[Unknown]
    reveal_type(incorrect_default_dict)  # revealed: defaultdict[Unknown, Unknown]
    reveal_type(incorrect_default_dict2)  # revealed: defaultdict[Unknown, Unknown]
    reveal_type(incorrect_deque)  # revealed: deque[Unknown]
    reveal_type(incorrect_ordered_dict)  # revealed: OrderedDict[Unknown, Unknown]
    reveal_type(incorrect_ordered_dict2)  # revealed: OrderedDict[Unknown, Unknown]
```

## Inheritance

The aliases can be inherited from. Some of these are still partially or wholly TODOs.

```py
import typing
from ty_extensions import reveal_mro

####################
### Built-ins
####################

class ListSubclass(typing.List): ...

# revealed: (<class 'ListSubclass'>, <class 'list[Unknown]'>, <class 'MutableSequence[Unknown]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(ListSubclass)

class DictSubclass(typing.Dict): ...

# revealed: (<class 'DictSubclass'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(DictSubclass)

class SetSubclass(typing.Set): ...

# revealed: (<class 'SetSubclass'>, <class 'set[Unknown]'>, <class 'MutableSet[Unknown]'>, <class 'AbstractSet[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(SetSubclass)

class FrozenSetSubclass(typing.FrozenSet): ...

# revealed: (<class 'FrozenSetSubclass'>, <class 'frozenset[Unknown]'>, <class 'AbstractSet[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(FrozenSetSubclass)

####################
### `collections`
####################

class ChainMapSubclass(typing.ChainMap): ...

# revealed: (<class 'ChainMapSubclass'>, <class 'ChainMap[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(ChainMapSubclass)

class CounterSubclass(typing.Counter): ...

# revealed: (<class 'CounterSubclass'>, <class 'Counter[Unknown]'>, <class 'dict[Unknown, int]'>, <class 'MutableMapping[Unknown, int]'>, <class 'Mapping[Unknown, int]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(CounterSubclass)

class DefaultDictSubclass(typing.DefaultDict): ...

# revealed: (<class 'DefaultDictSubclass'>, <class 'defaultdict[Unknown, Unknown]'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(DefaultDictSubclass)

class DequeSubclass(typing.Deque): ...

# revealed: (<class 'DequeSubclass'>, <class 'deque[Unknown]'>, <class 'MutableSequence[Unknown]'>, <class 'Sequence[Unknown]'>, <class 'Reversible[Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(DequeSubclass)

class OrderedDictSubclass(typing.OrderedDict): ...

# revealed: (<class 'OrderedDictSubclass'>, <class 'OrderedDict[Unknown, Unknown]'>, <class 'dict[Unknown, Unknown]'>, <class 'MutableMapping[Unknown, Unknown]'>, <class 'Mapping[Unknown, Unknown]'>, <class 'Collection[Unknown]'>, <class 'Iterable[Unknown]'>, <class 'Container[Unknown]'>, typing.Protocol, typing.Generic, <class 'object'>)
reveal_mro(OrderedDictSubclass)
```
