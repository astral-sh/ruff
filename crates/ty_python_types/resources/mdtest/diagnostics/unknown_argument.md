# Unknown argument diagnostics

<!-- snapshot-diagnostics -->

If a non-union callable is called with a parameter that doesn't match any parameter from the
signature, we add a subdiagnostic showing where the callable was defined. We don't do this for
unions as we currently emit a separate diagnostic for each element of the union; having a
sub-diagnostic for each element would probably be too verbose for it to be worth it.

`module.py`:

```py
def f(a, b, c=42): ...
def g(a, b): ...

class Foo:
    def method(self, a, b): ...
```

`main.py`:

```py
from module import f, g, Foo

f(a=1, b=2, c=3, d=42)  # error: [unknown-argument]

def coinflip() -> bool:
    return True

h = f if coinflip() else g

# error: [unknown-argument]
# error: [unknown-argument]
h(a=1, b=2, d=42)

Foo().method(a=1, b=2, c=3)  # error: [unknown-argument]
```
