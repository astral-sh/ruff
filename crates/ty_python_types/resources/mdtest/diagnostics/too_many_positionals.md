# too-many-positional-arguments diagnostics

<!-- snapshot-diagnostics -->

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

f(1, 2, 3)  # error: [too-many-positional-arguments]

def coinflip() -> bool:
    return True

h = f if coinflip() else g

# error: [too-many-positional-arguments]
# error: [too-many-positional-arguments]
h(1, 2, 3)

Foo().method(1, 2)  # error: [too-many-positional-arguments]
```
