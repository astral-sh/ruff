from __future__ import annotations

def foo(var: "MyClass") -> MyClass:
    x: "MyClass"

def foo (*, inplace: "bool"):
    pass

def foo(*args: "str", **kwargs: "int"):
    pass

x: Tuple["MyClass"]

x: Callable[["MyClass"], None]
