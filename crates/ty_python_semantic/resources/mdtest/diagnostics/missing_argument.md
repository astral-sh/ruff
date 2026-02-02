# Missing argument diagnostics

<!-- snapshot-diagnostics -->

If a non-union callable is called with a required parameter missing, we add a subdiagnostic showing
where the parameter was defined. We don't do this for unions as we currently emit a separate
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

f()  # error: [missing-argument]

def coinflip() -> bool:
    return True

h = f if coinflip() else g

# error: [missing-argument]
# error: [missing-argument]
h(b=56)

Foo().method()  # error: [missing-argument]
```
