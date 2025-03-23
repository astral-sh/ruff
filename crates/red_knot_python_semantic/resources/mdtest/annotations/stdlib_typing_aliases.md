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
    chain_map_parametrized: typing.ChainMap[int],
    counter_bare: typing.Counter,
    counter_parametrized: typing.Counter[int],
    default_dict_bare: typing.DefaultDict,
    default_dict_parametrized: typing.DefaultDict[str, int],
    deque_bare: typing.Deque,
    deque_parametrized: typing.Deque[str],
    ordered_dict_bare: typing.OrderedDict,
    ordered_dict_parametrized: typing.OrderedDict[int, str],
):
    reveal_type(list_bare)  # revealed: list
    reveal_type(list_parametrized)  # revealed: list

    reveal_type(dict_bare)  # revealed: dict
    reveal_type(dict_parametrized)  # revealed: dict

    reveal_type(set_bare)  # revealed: set
    reveal_type(set_parametrized)  # revealed: set

    reveal_type(frozen_set_bare)  # revealed: frozenset
    reveal_type(frozen_set_parametrized)  # revealed: frozenset

    reveal_type(chain_map_bare)  # revealed: ChainMap
    reveal_type(chain_map_parametrized)  # revealed: ChainMap

    reveal_type(counter_bare)  # revealed: Counter
    reveal_type(counter_parametrized)  # revealed: Counter

    reveal_type(default_dict_bare)  # revealed: defaultdict
    reveal_type(default_dict_parametrized)  # revealed: defaultdict

    reveal_type(deque_bare)  # revealed: deque
    reveal_type(deque_parametrized)  # revealed: deque

    reveal_type(ordered_dict_bare)  # revealed: OrderedDict
    reveal_type(ordered_dict_parametrized)  # revealed: OrderedDict
```

## Inheritance

The aliases can be inherited from. Some of these are still partially or wholly TODOs.

```py
import typing

####################
### Built-ins

class ListSubclass(typing.List): ...

# revealed: tuple[Literal[ListSubclass], Literal[list], Literal[MutableSequence], Literal[Sequence], Literal[Reversible], Literal[Collection], Literal[Iterable], Literal[Container], @Todo(protocol), Literal[object]]
reveal_type(ListSubclass.__mro__)

class DictSubclass(typing.Dict): ...

# TODO: should have `Generic`, should not have `Unknown`
# revealed: tuple[Literal[DictSubclass], Literal[dict], Unknown, Literal[object]]
reveal_type(DictSubclass.__mro__)

class SetSubclass(typing.Set): ...

# revealed: tuple[Literal[SetSubclass], Literal[set], Literal[MutableSet], Literal[AbstractSet], Literal[Collection], Literal[Iterable], Literal[Container], @Todo(protocol), Literal[object]]
reveal_type(SetSubclass.__mro__)

class FrozenSetSubclass(typing.FrozenSet): ...

# TODO: should have `Generic`, should not have `Unknown`
# revealed: tuple[Literal[FrozenSetSubclass], Literal[frozenset], Unknown, Literal[object]]
reveal_type(FrozenSetSubclass.__mro__)

####################
### `collections`

class ChainMapSubclass(typing.ChainMap): ...

# TODO: Should be (ChainMapSubclass, ChainMap, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[Literal[ChainMapSubclass], Literal[ChainMap], Unknown, Literal[object]]
reveal_type(ChainMapSubclass.__mro__)

class CounterSubclass(typing.Counter): ...

# TODO: Should be (CounterSubclass, Counter, dict, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[Literal[CounterSubclass], Literal[Counter], Unknown, Literal[object]]
reveal_type(CounterSubclass.__mro__)

class DefaultDictSubclass(typing.DefaultDict): ...

# TODO: Should be (DefaultDictSubclass, defaultdict, dict, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[Literal[DefaultDictSubclass], Literal[defaultdict], Unknown, Literal[object]]
reveal_type(DefaultDictSubclass.__mro__)

class DequeSubclass(typing.Deque): ...

# revealed: tuple[Literal[DequeSubclass], Literal[deque], Literal[MutableSequence], Literal[Sequence], Literal[Reversible], Literal[Collection], Literal[Iterable], Literal[Container], @Todo(protocol), Literal[object]]
reveal_type(DequeSubclass.__mro__)

class OrderedDictSubclass(typing.OrderedDict): ...

# TODO: Should be (OrderedDictSubclass, OrderedDict, dict, MutableMapping, Mapping, Collection, Sized, Iterable, Container, Generic, object)
# revealed: tuple[Literal[OrderedDictSubclass], Literal[OrderedDict], Unknown, Literal[object]]
reveal_type(OrderedDictSubclass.__mro__)
```
