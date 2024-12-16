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

class _List(typing.List): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(_List.__mro__)  # revealed: tuple[Literal[_List], Literal[list], Unknown, Literal[object]]

class _Dict(typing.Dict): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(_Dict.__mro__)  # revealed: tuple[Literal[_Dict], Literal[dict], Unknown, Literal[object]]

class _Set(typing.Set): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(_Set.__mro__)  # revealed: tuple[Literal[_Set], Literal[set], Unknown, Literal[object]]

class _FrozenSet(typing.FrozenSet): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(_FrozenSet.__mro__)  # revealed: tuple[Literal[_FrozenSet], Literal[frozenset], Unknown, Literal[object]]

####################
### `collections`

class _ChainMap(typing.ChainMap): ...

# revealed: tuple[Literal[_ChainMap], @Todo(Support for more typing aliases as base classes), Literal[object]]
reveal_type(_ChainMap.__mro__)

class _Counter(typing.Counter): ...

# revealed: tuple[Literal[_Counter], @Todo(Support for more typing aliases as base classes), Literal[object]]
reveal_type(_Counter.__mro__)

class _DefaultDict(typing.DefaultDict): ...

# revealed: tuple[Literal[_DefaultDict], @Todo(Support for more typing aliases as base classes), Literal[object]]
reveal_type(_DefaultDict.__mro__)

class _Deque(typing.Deque): ...

# revealed: tuple[Literal[_Deque], @Todo(Support for more typing aliases as base classes), Literal[object]]
reveal_type(_Deque.__mro__)

class _OrderedDict(typing.OrderedDict): ...

# revealed: tuple[Literal[_OrderedDict], @Todo(Support for more typing aliases as base classes), Literal[object]]
reveal_type(_OrderedDict.__mro__)
```
