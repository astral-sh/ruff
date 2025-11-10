from typing import Annotated, Optional, TypeAlias, Union

# Errors
a: Union[Annotated[int, "An integer"], None] = 5
b: Annotated[Union[int, str], "An integer or string"] | None = "World"
c: Optional[Annotated[float, "A float"]] = 2.71
d: Union[Annotated[int, "An integer"], Annotated[str, "A string"]]

def f1(x: Union[Annotated[int, "An integer"], None]) -> None:
    pass

def f2(y: Annotated[Union[int, str], "An integer or string"] | None) -> None:
    pass

def f3(z: Optional[Annotated[float, "A float"]]) -> None:
    pass

e: Annotated[Annotated[int, "An integer"], "Another annotation"]

l: TypeAlias = Annotated[int, "An integer"] | None
m: TypeAlias = Union[Annotated[int, "An integer"], str]
n: TypeAlias = Optional[Annotated[float, "A float"]]
o: TypeAlias = "Annotated[str, 'A string'] | None"

p: None | Annotated[int, "An integer"]

q = Annotated[int | str, "An integer or string"] | None
r = Optional[Annotated[float | None, "A float or None"]]

@some_decorator()
def func(x: Annotated[str, "A string"] | None) -> None:
    pass


# OK
g: Annotated[Union[int, str], "An integer or string"] = 42
h: Annotated[Union[float, None], "A float or None"] = 3.14

def f4(x: Annotated[Union[int, str], "An integer or string"]) -> None:
    pass

def f5(y: Annotated[Union[float, None], "A float or None"]) -> None:
    pass

def f6(z: Annotated[str | None, func()]):
    pass

i: Union[int, str] = 7
j: Optional[float] = None
k: Annotated[int, "An integer"] = 10
