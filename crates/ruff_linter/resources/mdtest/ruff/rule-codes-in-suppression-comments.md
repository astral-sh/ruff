# `rule-codes-in-suppression-comments` (`RUF106`)

```toml
[lint]
preview = true
select = ["RUF106"]
external = ["EXT"]
```

## `ruff:ignore`

Each Ruff rule code receives a separate diagnostic. Rule names and external or unknown codes are
preserved:

```py
# snapshot: rule-codes-in-suppression-comments
# snapshot: rule-codes-in-suppression-comments
# ruff:ignore[F401, undefined-name, EXT001, UNKNOWN, F841]
value = 1
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:15
  |
3 | # ruff:ignore[F401, undefined-name, EXT001, UNKNOWN, F841]
  |               ^^^^
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:ignore[F401, undefined-name, EXT001, UNKNOWN, F841]
3 + # ruff:ignore[unused-import, undefined-name, EXT001, UNKNOWN, F841]
4 | value = 1
  |


error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:54
  |
3 | # ruff:ignore[F401, undefined-name, EXT001, UNKNOWN, F841]
  |                                                      ^^^^
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:ignore[F401, undefined-name, EXT001, UNKNOWN, F841]
3 + # ruff:ignore[F401, undefined-name, EXT001, UNKNOWN, unused-variable]
4 | value = 1
  |
```

Valid human-readable names are unaffected:

```py
# snapshot: rule-codes-in-suppression-comments
# snapshot: rule-codes-in-suppression-comments
# ruff:ignore[F401, undefined-name, F841]
value = 1
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:7:15
  |
7 | # ruff:ignore[F401, undefined-name, F841]
  |               ^^^^
help: Replace rule code with name
  |
6 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:ignore[F401, undefined-name, F841]
7 + # ruff:ignore[unused-import, undefined-name, F841]
8 | value = 1
  |


error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:7:37
  |
7 | # ruff:ignore[F401, undefined-name, F841]
  |                                     ^^^^
help: Replace rule code with name
  |
6 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:ignore[F401, undefined-name, F841]
7 + # ruff:ignore[F401, undefined-name, unused-variable]
8 | value = 1
  |
```

## `ruff:file-ignore`

```py
# snapshot: rule-codes-in-suppression-comments
# snapshot: rule-codes-in-suppression-comments
# ruff:file-ignore[F401, F841]
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:20
  |
3 | # ruff:file-ignore[F401, F841]
  |                    ^^^^
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:file-ignore[F401, F841]
3 + # ruff:file-ignore[unused-import, F841]
  |


error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:26
  |
3 | # ruff:file-ignore[F401, F841]
  |                          ^^^^
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:file-ignore[F401, F841]
3 + # ruff:file-ignore[F401, unused-variable]
  |
```

## Matched `ruff:disable` and `ruff:enable`

Matching comments are reported and fixed together:

```py
# snapshot: rule-codes-in-suppression-comments
# snapshot: rule-codes-in-suppression-comments
# ruff:disable[F401, undefined-name, F841]
value = 1
# ruff:enable[F401, undefined-name, F841]
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:16
  |
3 | # ruff:disable[F401, undefined-name, F841]
  |                ^^^^
4 | value = 1
5 | # ruff:enable[F401, undefined-name, F841]
  |               ----
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:disable[F401, undefined-name, F841]
3 + # ruff:disable[unused-import, undefined-name, F841]
4 | value = 1
  - # ruff:enable[F401, undefined-name, F841]
5 + # ruff:enable[unused-import, undefined-name, F841]
  |


error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:38
  |
3 | # ruff:disable[F401, undefined-name, F841]
  |                                      ^^^^
4 | value = 1
5 | # ruff:enable[F401, undefined-name, F841]
  |                                     ----
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:disable[F401, undefined-name, F841]
3 + # ruff:disable[F401, undefined-name, unused-variable]
4 | value = 1
  - # ruff:enable[F401, undefined-name, F841]
5 + # ruff:enable[F401, undefined-name, unused-variable]
  |
```

## Unmatched `ruff:disable`

An unmatched disable comment is still an effective suppression through the end of its indentation
level:

```py
# snapshot: rule-codes-in-suppression-comments
# ruff:disable[F401]
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:2:16
  |
2 | # ruff:disable[F401]
  |                ^^^^
help: Replace rule code with name
  |
1 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:disable[F401]
2 + # ruff:disable[unused-import]
  |
```

## Unmatched `ruff:enable`

An unmatched enable comment is invalid and is left to `invalid-suppression-comment`:

```py
# ruff:enable[F401]
```

## Redirected codes

Redirected codes are replaced with the name of their canonical rule:

```py
# snapshot: rule-codes-in-suppression-comments
# ruff:ignore[PGH001]
value = 1
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:2:15
  |
2 | # ruff:ignore[PGH001]
  |               ^^^^^^
help: Replace rule code with name
  |
1 | # snapshot: rule-codes-in-suppression-comments
  - # ruff:ignore[PGH001]
2 + # ruff:ignore[suspicious-eval-usage]
3 | value = 1
  |
```

## Nested suppression comments

Only the rule codes within a nested suppression comment are replaced:

```py
# snapshot: rule-codes-in-suppression-comments
# snapshot: rule-codes-in-suppression-comments
value = 1  # explanation # ruff:ignore[F401, F841] reason # another
```

```snapshot
error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:40
  |
3 | value = 1  # explanation # ruff:ignore[F401, F841] reason # another
  |                                        ^^^^
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - value = 1  # explanation # ruff:ignore[F401, F841] reason # another
3 + value = 1  # explanation # ruff:ignore[unused-import, F841] reason # another
  |


error[RUF106]: Rule code used instead of name in suppression comment
 --> src/mdtest_snippet.py:3:46
  |
3 | value = 1  # explanation # ruff:ignore[F401, F841] reason # another
  |                                              ^^^^
help: Replace rule code with name
  |
2 | # snapshot: rule-codes-in-suppression-comments
  - value = 1  # explanation # ruff:ignore[F401, F841] reason # another
3 + value = 1  # explanation # ruff:ignore[F401, unused-variable] reason # another
  |
```

## Comments without Ruff rule codes

Comments containing only names and external or unknown codes are unchanged:

```py
# ruff:ignore[unused-import, EXT001, UNKNOWN]
value = 1
```

## Self-suppression

The rule can be suppressed by its code or name:

```py
# ruff:ignore[F401, RUF106]
value = 1
```

```py
# ruff:ignore[F401, rule-codes-in-suppression-comments]
value = 1
```

The diagnostic can also be suppressed with a `noqa` comment:

```py
value = 1  # ruff:ignore[F401]  # noqa: RUF106
```
