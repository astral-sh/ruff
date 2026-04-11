# too-many-positional-arguments diagnostics

If a non-union callable is called with too many positional arguments, we add a subdiagnostic showing
where the callable was defined. We don't do this for unions as we currently emit a separate
diagnostic for each element of the union; having a sub-diagnostic for each element would probably be
too verbose for it to be worth it.

`module.py`:

```py
def f(a, b=42): ...
def g(a, b): ...

class Foo:
    def method(self, a): ...
```

`main.py`:

```py
from module import f, g, Foo

# snapshot
f(1, 2, 3)  # error: [too-many-positional-arguments]

def coinflip() -> bool:
    return True

h = f if coinflip() else g

# snapshot
# error: [too-many-positional-arguments]
# error: [too-many-positional-arguments]
h(1, 2, 3)

# snapshot
Foo().method(1, 2)  # error: [too-many-positional-arguments]
```

```diagnostics
error[too-many-positional-arguments]: Too many positional arguments to function `f`: expected 2, got 3
 --> src/main.py:4:9
  |
3 | # snapshot
4 | f(1, 2, 3)  # error: [too-many-positional-arguments]
  |         ^
5 |
6 | def coinflip() -> bool:
  |
info: Function signature here
 --> src/module.py:1:5
  |
1 | def f(a, b=42): ...
  |     ^^^^^^^^^^
2 | def g(a, b): ...
  |


error[too-many-positional-arguments]: Too many positional arguments to function `f`: expected 2, got 3
  --> src/main.py:14:9
   |
12 | # error: [too-many-positional-arguments]
13 | # error: [too-many-positional-arguments]
14 | h(1, 2, 3)
   |         ^
15 |
16 | # snapshot
   |
info: Union variant `def f(a, b=42) -> Unknown` is incompatible with this call site
info: Attempted to call union type `(def f(a, b=42) -> Unknown) | (def g(a, b) -> Unknown)`


error[too-many-positional-arguments]: Too many positional arguments to function `g`: expected 2, got 3
  --> src/main.py:14:9
   |
12 | # error: [too-many-positional-arguments]
13 | # error: [too-many-positional-arguments]
14 | h(1, 2, 3)
   |         ^
15 |
16 | # snapshot
   |
info: Union variant `def g(a, b) -> Unknown` is incompatible with this call site
info: Attempted to call union type `(def f(a, b=42) -> Unknown) | (def g(a, b) -> Unknown)`


error[too-many-positional-arguments]: Too many positional arguments to bound method `method`: expected 2, got 3
  --> src/main.py:17:17
   |
16 | # snapshot
17 | Foo().method(1, 2)  # error: [too-many-positional-arguments]
   |                 ^
   |
info: Method signature here
 --> src/module.py:5:9
  |
4 | class Foo:
5 |     def method(self, a): ...
  |         ^^^^^^^^^^^^^^^
  |
```
