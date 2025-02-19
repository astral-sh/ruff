# Special cases for int/float/complex in annotations

In order to support common use cases, an annotation of `float` actually means `int | float`, and an
annotation of `complex` actually means `int | float | complex`. See
[the specification](https://typing.readthedocs.io/en/latest/spec/special-types.html#special-cases-for-float-and-complex)

## float

An annotation of `float` means `int | float`, so `int` is assignable to it:

```py
def takes_float(x: float):
    pass

def passes_int_to_float(x: int):
    # no error!
    takes_float(x)
```

It also applies to variable annotations:

```py
def assigns_int_to_float(x: int):
    # no error!
    y: float = x
```

It doesn't work the other way around:

```py
def takes_int(x: int):
    pass

def passes_float_to_int(x: float):
    # error: [invalid-argument-type]
    takes_int(x)

def assigns_float_to_int(x: float):
    # error: [invalid-assignment]
    y: int = x
```

Unlike other type checkers, we choose not to obfuscate this special case by displaying `int | float`
as just `float`; we display the actual type:

```py
def f(x: float):
    reveal_type(x)  # revealed: int | float
```

## complex

An annotation of `complex` means `int | float | complex`, so `int` and `float` are both assignable
to it (but not the other way around):

```py
def takes_complex(x: complex):
    pass

def passes_to_complex(x: float, y: int):
    # no errors!
    takes_complex(x)
    takes_complex(y)

def assigns_to_complex(x: float, y: int):
    # no errors!
    a: complex = x
    b: complex = y

def takes_int(x: int):
    pass

def takes_float(x: float):
    pass

def passes_complex(x: complex):
    # error: [invalid-argument-type]
    takes_int(x)
    # error: [invalid-argument-type]
    takes_float(x)

def assigns_complex(x: complex):
    # error: [invalid-assignment]
    y: int = x
    # error: [invalid-assignment]
    z: float = x

def f(x: complex):
    reveal_type(x)  # revealed: int | float | complex
```
