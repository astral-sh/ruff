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

f()  # snapshot: missing-argument

def coinflip() -> bool:
    return True

h = f if coinflip() else g

# snapshot: missing-argument
# snapshot: missing-argument
h(b=56)

Foo().method()  # snapshot: missing-argument
```

```snapshot
error[missing-argument]: No argument provided for required parameter `a` of function `f`
 --> src/main.py:3:1
  |
1 | from module import f, g, Foo
2 |
3 | f()  # snapshot: missing-argument
  | ^^^
4 |
5 | def coinflip() -> bool:
  |
info: Parameter declared here
 --> src/module.py:1:7
  |
1 | def f(a, b=42): ...
  |       ^
2 | def g(a, b): ...
  |


error[missing-argument]: No argument provided for required parameter `a` of function `f`
  --> src/main.py:12:1
   |
10 | # snapshot: missing-argument
11 | # snapshot: missing-argument
12 | h(b=56)
   | ^^^^^^^
13 |
14 | Foo().method()  # snapshot: missing-argument
   |
info: Union variant `def f(a, b=42) -> Unknown` is incompatible with this call site
info: Attempted to call union type `(def f(a, b=42) -> Unknown) | (def g(a, b) -> Unknown)`


error[missing-argument]: No argument provided for required parameter `a` of function `g`
  --> src/main.py:12:1
   |
10 | # snapshot: missing-argument
11 | # snapshot: missing-argument
12 | h(b=56)
   | ^^^^^^^
13 |
14 | Foo().method()  # snapshot: missing-argument
   |
info: Union variant `def g(a, b) -> Unknown` is incompatible with this call site
info: Attempted to call union type `(def f(a, b=42) -> Unknown) | (def g(a, b) -> Unknown)`


error[missing-argument]: No argument provided for required parameter `a` of bound method `method`
  --> src/main.py:14:1
   |
12 | h(b=56)
13 |
14 | Foo().method()  # snapshot: missing-argument
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
