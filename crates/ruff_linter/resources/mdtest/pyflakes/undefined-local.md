# `undefined-local` (`F823`)

```toml
[lint]
select = ["F823"]
```

## Multiple assignments

Regression tests for <https://github.com/astral-sh/ruff/issues/30224>.

```py
my_var = 0

def f():
    print(my_var)  # error: [undefined-local]
    my_var = 1
    my_var = 2

def f():
    my_var += 1  # error: [undefined-local]
    my_var += 1

# OK: the read comes after the first local binding.
def f():
    my_var = 1
    print(my_var)
    my_var = 2
```
