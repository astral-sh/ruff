# `forward-annotation-syntax-error` (`F722`)

```toml
target-version = "py312"

[lint]
select = ["F722"]
```

## Parse errors

Quoted annotations must parse as Python expressions.

```py
# error: [forward-annotation-syntax-error] "Expected an expression"
invalid: "/"
```

## Semantic syntax errors

An expression can parse successfully but still contain a semantic syntax error.

```py
# error: [forward-annotation-syntax-error] "Duplicate parameter"
invalid: "(lambda x, x: 0)"
```

## Semantic syntax errors currently mapped to disabled lint rules

`F722` reports semantic syntax errors even when their overlapping lint rules are disabled, in this
case `yield-outside-function` (`F704`).

```py
# error: [forward-annotation-syntax-error] "`yield` statement outside of a function"
invalid: "(yield 1)"
```

## Semantic syntax errors currently mapped to enabled lint rules

Disabling `F722` suppresses the semantic syntax error even when `F704` remains enabled.

```toml
target-version = "py312"

[lint]
select = ["F704"]
```

```py
invalid: "(yield 1)"
```
