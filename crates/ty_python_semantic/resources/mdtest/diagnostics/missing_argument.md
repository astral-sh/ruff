# Missing argument diagnostics

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

# snapshot
f()  # error: [missing-argument]

def coinflip() -> bool:
    return True

h = f if coinflip() else g

# snapshot
# error: [missing-argument]
# error: [missing-argument]
h(b=56)

# snapshot
Foo().method()  # error: [missing-argument]
```

```diagnostics
error[missing-argument]: No argument provided for required parameter `a` of function `f`
 --> src/main.py:4:1
  |
3 | # snapshot
4 | f()  # error: [missing-argument]
  | ^^^
5 |
6 | def coinflip() -> bool:
  |
info: Parameter declared here
 --> src/module.py:1:7
  |
1 | def f(a, b=42): ...
  |       ^
2 | def g(a, b): ...
  |


error[missing-argument]: No argument provided for required parameter `a` of function `f`
  --> src/main.py:14:1
   |
12 | # error: [missing-argument]
13 | # error: [missing-argument]
14 | h(b=56)
   | ^^^^^^^
15 |
16 | # snapshot
   |
info: Union variant `def f(a, b=42) -> Unknown` is incompatible with this call site
info: Attempted to call union type `(def f(a, b=42) -> Unknown) | (def g(a, b) -> Unknown)`


error[missing-argument]: No argument provided for required parameter `a` of function `g`
  --> src/main.py:14:1
   |
12 | # error: [missing-argument]
13 | # error: [missing-argument]
14 | h(b=56)
   | ^^^^^^^
15 |
16 | # snapshot
   |
info: Union variant `def g(a, b) -> Unknown` is incompatible with this call site
info: Attempted to call union type `(def f(a, b=42) -> Unknown) | (def g(a, b) -> Unknown)`


error[missing-argument]: No argument provided for required parameter `a` of bound method `method`
  --> src/main.py:17:1
   |
16 | # snapshot
17 | Foo().method()  # error: [missing-argument]
   | ^^^^^^^^^^^^^^
   |
info: Parameter declared here
 --> src/module.py:5:22
  |
4 | class Foo:
5 |     def method(self, a): ...
  |                      ^
  |
```
