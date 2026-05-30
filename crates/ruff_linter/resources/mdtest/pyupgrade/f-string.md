# `f-string` (`UP032`)

```toml
lint.select = ["UP032"]
```

## A dropped leading empty literal leaves no orphan whitespace

An empty literal in an implicit string concatenation contributes nothing, so dropping it must not
leave stray whitespace before the f-string.

```py
"" "{}".format(x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:1:1
  |
1 | "" "{}".format(x)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
  - "" "{}".format(x)  # snapshot: f-string
1 + f"{x}"  # snapshot: f-string
2 | "a" "" "{}".format(x)  # snapshot: f-string
```

```py
"a" "" "{}".format(x)  # snapshot: f-string
```

```snapshot
error[UP032]: Use f-string instead of `format` call
 --> src/mdtest_snippet.py:2:1
  |
2 | "a" "" "{}".format(x)  # snapshot: f-string
  | ^^^^^^^^^^^^^^^^^^^^^
  |
help: Convert to f-string
1 | "" "{}".format(x)  # snapshot: f-string
  - "a" "" "{}".format(x)  # snapshot: f-string
2 + "a" f"{x}"  # snapshot: f-string
```
