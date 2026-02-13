# `ParamSpec` regression on 3.9

```toml
[environment]
python-version = "3.9"
```

This used to panic when run on Python 3.9 because `ParamSpec` was introduced in Python 3.10 and the
diagnostic message for `invalid-exception-caught` expects to construct `typing.ParamSpec`.

```py
# error: [invalid-syntax]
def foo[**P]() -> None:
    try:
        pass
    # error: [invalid-exception-caught] "Invalid object caught in an exception handler: Object has type `ParamSpec`"
    except P:
        pass
```
