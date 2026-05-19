# Version-related syntax error diagnostics

## `match` statement

The `match` statement was introduced in Python 3.10.

### Before 3.10

<!-- snapshot-diagnostics -->

We should emit a syntax error before 3.10.

```toml
[environment]
python-version = "3.9"
```

```py
match 2:  # error: 1 [invalid-syntax] "Cannot use `match` statement on Python 3.9 (syntax was added in Python 3.10)"
    case 1:
        print("it's one")
```

### After 3.10

On or after 3.10, no error should be reported.

```toml
[environment]
python-version = "3.10"
```

```py
match 2:
    case 1:
        print("it's one")
```

## PEP 695 type parameter lists

PEP 695 type parameter lists were introduced in Python 3.12. Even though the syntax is invalid on
older Python versions, we should still handle later semantic analysis gracefully.

```toml
[environment]
python-version = "3.9"
```

```py
# error: 8 [invalid-syntax] "Cannot use type parameter lists on Python 3.9 (syntax was added in Python 3.12)"
class C[**P, **Q, *Ts]:
    pass

# error: 1 [invalid-type-arguments] "No type arguments provided for required type variables `Q`, `Ts` of class `C`"
# error: 3 [invalid-type-arguments] "Type argument for `ParamSpec` must be either a list of types, `ParamSpec`, `Concatenate`, or `...`"
C[0]

# error: 1 [invalid-syntax] "Cannot use `type` alias statement on Python 3.9 (syntax was added in Python 3.12)"
type Alias[*Ts, **P] = int
Alias[MissingAliasArg]  # error: [unresolved-reference]

# error: 8 [invalid-syntax] "Cannot use type parameter lists on Python 3.9 (syntax was added in Python 3.12)"
class D[*Ts, **P]:
    pass

D[MissingClassArg]  # error: [unresolved-reference]

# error: 8 [invalid-syntax] "Cannot use type parameter lists on Python 3.9 (syntax was added in Python 3.12)"
class E[**P, **Q, *Ts]:
    pass

# error: 1 [invalid-type-arguments] "No type arguments provided for required type variables `Q`, `Ts` of class `E`"
E[MissingClassArg2]  # error: [unresolved-reference]
```
