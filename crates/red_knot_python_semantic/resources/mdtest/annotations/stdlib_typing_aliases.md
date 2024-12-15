# Typing-module aliases to other stdlib classes

The `typing` module has various aliases to other stdlib classes. These are a legacy feature, but
still need to be supported by a type checker.

## Currently unsupported

Support for most of these symbols is currently a TODO:

```py
import typing

def f(
    a: typing.List,
    b: typing.List[int],
    c: typing.Dict,
    d: typing.Dict[int, str],
    e: typing.DefaultDict,
    f: typing.DefaultDict[str, int],
    g: typing.Set,
    h: typing.Set[int],
    i: typing.FrozenSet,
    j: typing.FrozenSet[str],
    k: typing.OrderedDict,
    l: typing.OrderedDict[int, str],
    m: typing.Counter,
    n: typing.Counter[int],
):
    reveal_type(a)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(b)  # revealed: @Todo(typing.List alias)
    reveal_type(c)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(d)  # revealed: @Todo(typing.Dict alias)
    reveal_type(e)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(f)  # revealed: @Todo(typing.DefaultDict[] alias)
    reveal_type(g)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(h)  # revealed: @Todo(typing.Set alias)
    reveal_type(i)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(j)  # revealed: @Todo(typing.FrozenSet alias)
    reveal_type(k)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(l)  # revealed: @Todo(typing.OrderedDict alias)
    reveal_type(m)  # revealed: @Todo(Unsupported or invalid type in a type expression)
    reveal_type(n)  # revealed: @Todo(typing.Counter[] alias)
```

## Inheritance

The aliases can be inherited from. Some of these are still partially or wholly TODOs.

```py
import typing

class A(typing.Dict): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(A.__mro__)  # revealed: tuple[Literal[A], Literal[dict], Unknown, Literal[object]]

class B(typing.List): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(B.__mro__)  # revealed: tuple[Literal[B], Literal[list], Unknown, Literal[object]]

class C(typing.Set): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(C.__mro__)  # revealed: tuple[Literal[C], Literal[set], Unknown, Literal[object]]

class D(typing.FrozenSet): ...

# TODO: should have `Generic`, should not have `Unknown`
reveal_type(D.__mro__)  # revealed: tuple[Literal[D], Literal[frozenset], Unknown, Literal[object]]

class E(typing.DefaultDict): ...

reveal_type(E.__mro__)  # revealed: tuple[Literal[E], @Todo(Support for more typing aliases as base classes), Literal[object]]

class F(typing.OrderedDict): ...

reveal_type(F.__mro__)  # revealed: tuple[Literal[F], @Todo(Support for more typing aliases as base classes), Literal[object]]

class G(typing.Counter): ...

reveal_type(G.__mro__)  # revealed: tuple[Literal[G], @Todo(Support for more typing aliases as base classes), Literal[object]]
```
