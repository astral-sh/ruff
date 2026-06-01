# `f-string` (`UP032`)

```toml
lint.select = ["UP032"]
```

## Arguments that would misparse are parenthesized

In an f-string a leading `{` reads as an escaped brace and a lambda's `:` reads as the format-spec
separator, so we wrap either one in parentheses.

```py
"{}".format(lambda: 1)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:1:1
  |
1 | "{}".format(lambda: 1)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
  - "{}".format(lambda: 1)  # snapshot: f-string
1 + f"{(lambda: 1)}"  # snapshot: f-string
2 | "{}".format({} | {})  # snapshot: f-string
3 | "{.x}".format(lambda: 1)  # snapshot: f-string
```

```py
"{}".format({} | {})  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:2:1
  |
2 | "{}".format({} | {})  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | "{}".format(lambda: 1)  # snapshot: f-string
  - "{}".format({} | {})  # snapshot: f-string
2 + f"{({} | {})}"  # snapshot: f-string
3 | "{.x}".format(lambda: 1)  # snapshot: f-string
```

A lambda is wrapped even when the field has a trailing accessor:

```py
"{.x}".format(lambda: 1)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:3:1
  |
3 | "{.x}".format(lambda: 1)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | "{}".format(lambda: 1)  # snapshot: f-string
2 | "{}".format({} | {})  # snapshot: f-string
  - "{.x}".format(lambda: 1)  # snapshot: f-string
3 + f"{(lambda: 1).x}"  # snapshot: f-string
```
