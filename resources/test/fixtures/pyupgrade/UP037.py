from __future__ import annotations

def foo(var: "MyClass") -> "MyClass":
    x: "MyClass"

def foo (*, inplace: "bool"):
    pass

def foo(*args: "str", **kwargs: "int"):
    pass

x: Tuple["MyClass"]

x: Callable[["MyClass"], None]

class Foo(NamedTuple):
    x: "MyClass"

class D(TypedDict):
    E: TypedDict("E", foo="int", total=False)

class D(typing.TypedDict):
    E: TypedDict("E", {"foo": "int"})

class D(TypedDict):
    E: TypedDict("E", {"foo": "int"})

x: Annotated["str", "metadata"]

x: typing.Annotated["str", "metadata"]

x: Arg("str", "name")

x: DefaultArg("str", "name")

x: NamedArg("str", "name")

x: DefaultNamedArg("str", "name")

x: DefaultNamedArg("str", name="name")

x: VarArg("str")

x: List[List[List["MyClass"]]]

x: NamedTuple("X", [("foo", "int"), ("bar", "str")])

x: NamedTuple("X", fields=[("foo", "int"), ("bar", "str")])

x: NamedTuple(typename="X", fields=[("foo", "int")])

# These should NOT change
class D(TypedDict):
    E: TypedDict("E")

x: Annotated[()]

x: DefaultNamedArg(name="name", quox="str")

x: DefaultNamedArg(name="name")

x: NamedTuple("X", [("foo",), ("bar",)])

x: NamedTuple("X", ["foo", "bar"])

x: NamedTuple()

x: Literal["foo", "bar"]

x = TypeVar("x", "str")

x = cast(x, "str")

X = List["MyClass"]

X: MyCallable("X")


def foo(x, *args, **kwargs): ...

def foo(*, inplace): ...

x: Annotated[1:2] = ...
