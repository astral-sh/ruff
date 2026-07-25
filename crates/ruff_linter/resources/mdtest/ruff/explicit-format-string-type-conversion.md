# `explicit-format-string-type-conversion` (`RUF077`)

```toml
lint.preview = true
lint.select = ["RUF077"]
```

## `%`-formatting

### Basic examples

`repr()` and `ascii()` are replaced by the `%r` and `%a` conversions:

```py
foo = "foo"
bar = "bar"

"%s %s" % (repr(foo), bar)  # snapshot: explicit-format-string-type-conversion
"%s %s" % (foo, ascii(bar))  # error: [explicit-format-string-type-conversion]
```

```snapshot
error[RUF077]: Use `%r` instead of calling `repr()`
 --> src/mdtest_snippet.py:4:12
  |
4 | "%s %s" % (repr(foo), bar)  # snapshot: explicit-format-string-type-conversion
  |            ^^^^^^^^^
  |
help: Replace with `%r`
  |
3 |
  - "%s %s" % (repr(foo), bar)  # snapshot: explicit-format-string-type-conversion
4 + "%r %s" % (foo, bar)  # snapshot: explicit-format-string-type-conversion
5 | "%s %s" % (foo, ascii(bar))  # error: [explicit-format-string-type-conversion]
  |
```

`%s` already converts its value with `str()`, so an explicit `str()` call is simply dropped:

```py
"%s" % (str(foo),)  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Unnecessary `str()` call within a `%s` conversion
 --> src/mdtest_snippet.py:6:9
  |
6 | "%s" % (str(foo),)  # snapshot: explicit-format-string-type-conversion
  |         ^^^^^^^^
  |
help: Remove `str()` call
  |
5 | "%s %s" % (foo, ascii(bar))  # error: [explicit-format-string-type-conversion]
  - "%s" % (str(foo),)  # snapshot: explicit-format-string-type-conversion
6 + "%s" % (foo,)  # snapshot: explicit-format-string-type-conversion
7 | "%-10.3s" % (repr(foo),)  # error: [explicit-format-string-type-conversion]
  |
```

Flags, a width and a precision are all preserved:

```py
"%-10.3s" % (repr(foo),)  # error: [explicit-format-string-type-conversion]
"%#x %s" % (1, repr(foo))  # error: [explicit-format-string-type-conversion]
```

A `*` in the width or the precision consumes a value of its own:

```py
"%*.*s %s" % (10, 4, foo, repr(bar))  # error: [explicit-format-string-type-conversion]
```

### A single value

A right-hand side that isn't a tuple formats a single value, so the fix has to introduce the
tuple itself:

```py
foo = "foo"

"%s" % repr(foo)  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Use `%r` instead of calling `repr()`
 --> src/mdtest_snippet.py:3:8
  |
3 | "%s" % repr(foo)  # snapshot: explicit-format-string-type-conversion
  |        ^^^^^^^^^
  |
help: Replace with `%r`
  |
2 |
  - "%s" % repr(foo)  # snapshot: explicit-format-string-type-conversion
3 + "%r" % (foo,)  # snapshot: explicit-format-string-type-conversion
  |
```

### Mapping keys

```py
foo = "foo"

"%(foo)s" % {"foo": repr(foo)}  # snapshot: explicit-format-string-type-conversion
"%(foo)s %(foo)10s" % {"foo": repr(foo)}  # error: [explicit-format-string-type-conversion]
```

```snapshot
error[RUF077]: Use `%r` instead of calling `repr()`
 --> src/mdtest_snippet.py:3:21
  |
3 | "%(foo)s" % {"foo": repr(foo)}  # snapshot: explicit-format-string-type-conversion
  |                     ^^^^^^^^^
  |
help: Replace with `%r`
  |
2 |
  - "%(foo)s" % {"foo": repr(foo)}  # snapshot: explicit-format-string-type-conversion
3 + "%(foo)r" % {"foo": foo}  # snapshot: explicit-format-string-type-conversion
4 | "%(foo)s %(foo)10s" % {"foo": repr(foo)}  # error: [explicit-format-string-type-conversion]
  |
```

A key that is also formatted by a conversion other than `%s` is left alone, as is a key that
no conversion refers to:

```py
"%(foo)s %(foo)r" % {"foo": repr(foo)}
"%(bar)s" % {"foo": repr(foo), "bar": foo}
```

### Duplicate keys

Only the last entry written with a key survives, so rewriting one entry in isolation would
apply the conversion to a value that was never inspected. When every entry for the key applies
the same conversion to the same expression, rewriting them together is equivalent, and the fix
is offered as unsafe because a duplicated key is usually a mistake:

```py
foo = "foo"

# error: [explicit-format-string-type-conversion]
"%(foo)s" % {"foo": repr(foo), "foo": repr(foo)}  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Use `%r` instead of calling `repr()`
 --> src/mdtest_snippet.py:4:39
  |
4 | "%(foo)s" % {"foo": repr(foo), "foo": repr(foo)}  # snapshot: explicit-format-string-type-conversion
  |                                       ^^^^^^^^^
  |
help: Replace with `%r`
  |
3 | # error: [explicit-format-string-type-conversion]
  - "%(foo)s" % {"foo": repr(foo), "foo": repr(foo)}  # snapshot: explicit-format-string-type-conversion
4 + "%(foo)r" % {"foo": repr(foo), "foo": foo}  # snapshot: explicit-format-string-type-conversion
5 | foo = "foo"
  |
note: This is an unsafe fix and may change runtime behavior
```

When the entries differ — a different expression, a different conversion, or no conversion at
all — the diagnostic is reported without a fix:

```py
foo = "foo"
bar = "bar"

# error: [explicit-format-string-type-conversion]
"%(foo)s" % {"foo": repr(foo), "foo": repr(bar)}  # snapshot: explicit-format-string-type-conversion

# error: [explicit-format-string-type-conversion]
"%(foo)s" % {"foo": repr(foo), "foo": str(foo)}  # error: [explicit-format-string-type-conversion]

"%(foo)s" % {"foo": repr(foo), "foo": bar}  # error: [explicit-format-string-type-conversion]
```

```snapshot
error[RUF077]: Use `%r` instead of calling `repr()`
 --> src/mdtest_snippet.py:9:39
  |
9 | "%(foo)s" % {"foo": repr(foo), "foo": repr(bar)}  # snapshot: explicit-format-string-type-conversion
  |                                       ^^^^^^^^^
  |
help: Replace with `%r`
```

### Keys that aren't literals

A key whose value isn't known statically may overwrite an earlier entry, so entries before it
are reported without a fix. Entries after it are unaffected, since they win regardless:

```py
foo = "foo"
key = "foo"
mapping = {}

# No fix: `key` may be `"foo"`, and `mapping` may hold it.
"%(foo)s" % {"foo": repr(foo), key: "x"}  # snapshot: explicit-format-string-type-conversion
"%(foo)s" % {"foo": repr(foo), **mapping}  # error: [explicit-format-string-type-conversion]

# Fixable: the literal entry comes last, so nothing can overwrite it.
"%(foo)s" % {key: "x", "foo": repr(foo)}  # error: [explicit-format-string-type-conversion]
"%(foo)s" % {**mapping, "foo": repr(foo)}  # error: [explicit-format-string-type-conversion]
```

```snapshot
error[RUF077]: Use `%r` instead of calling `repr()`
 --> src/mdtest_snippet.py:6:21
  |
6 | "%(foo)s" % {"foo": repr(foo), key: "x"}  # snapshot: explicit-format-string-type-conversion
  |                     ^^^^^^^^^
  |
help: Replace with `%r`
```

### No errors

```py
foo = "foo"
bar = "bar"
values = [foo, bar]

# Already an `%r` or `%a` conversion.
"%r" % (repr(foo),)
"%a" % (ascii(foo),)

# Not a conversion that `repr()` can be folded into.
"%d" % (repr(foo),)

# Nothing to fold.
"%s" % (foo,)

# The number of values doesn't line up with the conversions.
"%s %s" % (repr(foo),)
"%s" % (repr(foo), bar)

# The values are unpacked.
"%s %s" % (*values,)

# `bytes` don't support the `%r` conversion.
b"%s" % (repr(foo),)

# Not the built-in `repr()`.
"%s" % (foo.repr(),)
"%s" % (repr(foo, bar),)
"%s" % (repr(),)
"%s" % (repr(*values),)
```

A shadowed built-in isn't the conversion function:

```py
def repr(value):
    return "!"


"%s" % (repr("foo"),)
```

## `str.format`

### Basic examples

```py
foo = "foo"
bar = "bar"

"{} {}".format(repr(foo), bar)  # snapshot: explicit-format-string-type-conversion
"{} {}".format(foo, ascii(bar))  # error: [explicit-format-string-type-conversion]
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
 --> src/mdtest_snippet.py:4:16
  |
4 | "{} {}".format(repr(foo), bar)  # snapshot: explicit-format-string-type-conversion
  |                ^^^^^^^^^
  |
help: Replace with `!r` conversion flag
  |
3 |
  - "{} {}".format(repr(foo), bar)  # snapshot: explicit-format-string-type-conversion
4 + "{!r} {}".format(foo, bar)  # snapshot: explicit-format-string-type-conversion
5 | "{} {}".format(foo, ascii(bar))  # error: [explicit-format-string-type-conversion]
  |
```

Unlike `%s`, `{}` formats its value with `format()` rather than `str()`, so an explicit
`str()` call becomes an explicit `!s` conversion:

```py
"{}".format(str(foo))  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Use the `!s` conversion flag instead of calling `str()`
 --> src/mdtest_snippet.py:6:13
  |
6 | "{}".format(str(foo))  # snapshot: explicit-format-string-type-conversion
  |             ^^^^^^^^
  |
help: Replace with `!s` conversion flag
  |
5 | "{} {}".format(foo, ascii(bar))  # error: [explicit-format-string-type-conversion]
  - "{}".format(str(foo))  # snapshot: explicit-format-string-type-conversion
6 + "{!s}".format(foo)  # snapshot: explicit-format-string-type-conversion
7 | "{:>10}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
  |
```

A format spec is preserved, and the conversion is inserted before it:

```py
"{:>10}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
 --> src/mdtest_snippet.py:7:17
  |
7 | "{:>10}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
  |                 ^^^^^^^^^
  |
help: Replace with `!r` conversion flag
  |
6 | "{}".format(str(foo))  # snapshot: explicit-format-string-type-conversion
  - "{:>10}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
7 + "{!r:>10}".format(foo)  # snapshot: explicit-format-string-type-conversion
8 | "{1} {0}".format(foo, repr(bar))  # error: [explicit-format-string-type-conversion]
  |
```

Explicit indices and keywords are both supported, including when a value is formatted more
than once:

```py
"{1} {0}".format(foo, repr(bar))  # error: [explicit-format-string-type-conversion]
"{foo}".format(foo=repr(foo))  # error: [explicit-format-string-type-conversion]
"{0} {0}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
  --> src/mdtest_snippet.py:10:18
   |
10 | "{0} {0}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
   |                  ^^^^^^^^^
   |
help: Replace with `!r` conversion flag
   |
9  | "{foo}".format(foo=repr(foo))  # error: [explicit-format-string-type-conversion]
   - "{0} {0}".format(repr(foo))  # snapshot: explicit-format-string-type-conversion
10 + "{0!r} {0!r}".format(foo)  # snapshot: explicit-format-string-type-conversion
11 | "{{}} {}".format(repr(foo))  # error: [explicit-format-string-type-conversion]
   |
```

Literal braces are not replacement fields:

```py
"{{}} {}".format(repr(foo))  # error: [explicit-format-string-type-conversion]
```

### Unpacked keywords

`**` unpacking is fine, as long as the field matches an explicit keyword:

```py
foo = "foo"
mapping = {}

"{foo}".format(**mapping, foo=repr(foo))  # error: [explicit-format-string-type-conversion]
"{bar}".format(**mapping, foo=repr(foo))
```

### No errors

```py
foo = "foo"
bar = "bar"
values = [foo, bar]


class Wrapper:
    attr = "attr"


# Already a conversion.
"{!r}".format(repr(foo))
"{!s:>10}".format(repr(foo))

# The field formats an attribute or an item of the value, not the value itself.
"{.attr}".format(repr(Wrapper()))
"{[0]}".format(repr(values))

# One of the two fields already converts.
"{0} {0!s}".format(repr(foo))

# The value isn't formatted at all.
"{}".format(foo, repr(bar))
"{foo}".format(foo=foo, bar=repr(bar))

# The arguments are unpacked.
"{}".format(*values)

# A nested field consumes an argument of its own.
"{:>{}}".format(repr(foo), 10)

# Mixing automatic and explicit field numbering is a `ValueError`.
"{} {0}".format(repr(foo))

# Not the built-in `repr()`.
"{}".format(foo.repr())
"{}".format(repr(foo, bar))
```

## Implicit concatenation

### Conversions in either part

The conversion may live in any part of an implicitly concatenated string:

```py
foo = "foo"
bar = "bar"

"%s " "%s" % (repr(foo), bar)  # error: [explicit-format-string-type-conversion]
("{} " "{}").format(foo, repr(bar))  # snapshot: explicit-format-string-type-conversion
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
 --> src/mdtest_snippet.py:5:26
  |
5 | ("{} " "{}").format(foo, repr(bar))  # snapshot: explicit-format-string-type-conversion
  |                          ^^^^^^^^^
  |
help: Replace with `!r` conversion flag
  |
4 | "%s " "%s" % (repr(foo), bar)  # error: [explicit-format-string-type-conversion]
  - ("{} " "{}").format(foo, repr(bar))  # snapshot: explicit-format-string-type-conversion
5 + ("{} " "{!r}").format(foo, bar)  # snapshot: explicit-format-string-type-conversion
  |
```

### Conversions split across parts

A conversion that straddles two parts can't be located in the source, so it is skipped:

```py
foo = "foo"

"%" "s" % (repr(foo),)
"{" "}".format(repr(foo))
```

## Escape sequences

Escapes that expand to a `%` or to a brace can't be located in the source, so they are
skipped, but ordinary escapes are unaffected:

```py
foo = "foo"

"\N{BULLET} %s" % (repr(foo),)  # error: [explicit-format-string-type-conversion]
"\t{}\n".format(repr(foo))  # error: [explicit-format-string-type-conversion]

# In a raw string, `\N{BULLET}` really is a replacement field named `BULLET`.
r"\N{BULLET}".format(BULLET=repr(foo))  # error: [explicit-format-string-type-conversion]

"\x25s" % (repr(foo),)
"\x7b}".format(repr(foo))
"\N{LEFT CURLY BRACKET}}".format(repr(foo))
```

## Fix safety

### Deleted comments

A comment that would be deleted by removing the call makes the fix unsafe:

```py
foo = "foo"

"{}".format(repr(  # snapshot: explicit-format-string-type-conversion
    # comment
    foo
))
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
 --> src/mdtest_snippet.py:3:13
  |
3 |   "{}".format(repr(  # snapshot: explicit-format-string-type-conversion
  |  _____________^
4 | |     # comment
5 | |     foo
6 | | ))
  | |_^
  |
help: Replace with `!r` conversion flag
  |
2 |
  - "{}".format(repr(  # snapshot: explicit-format-string-type-conversion
  -     # comment
  -     foo
  - ))
3 + "{!r}".format(foo)
  |
note: This is an unsafe fix and may change runtime behavior
```

### Preserved comments

Comments inside parentheses that wrap the argument are preserved, so the fix stays safe:

```py
foo = "foo"

"{}".format(repr((  # snapshot: explicit-format-string-type-conversion
    # comment
    foo
)))
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
 --> src/mdtest_snippet.py:3:13
  |
3 |   "{}".format(repr((  # snapshot: explicit-format-string-type-conversion
  |  _____________^
4 | |     # comment
5 | |     foo
6 | | )))
  | |__^
  |
help: Replace with `!r` conversion flag
  |
2 |
  - "{}".format(repr((  # snapshot: explicit-format-string-type-conversion
3 + "{!r}".format((  # snapshot: explicit-format-string-type-conversion
4 |     # comment
5 |     foo
  - )))
6 + ))
  |
```

## Several conversions in one expression

Each conversion call is reported separately. Their fixes touch disjoint ranges, so they all
apply in a single pass:

```py
foo = "foo"
bar = "bar"

# error: [explicit-format-string-type-conversion]
"%s %s" % (repr(foo), repr(bar))  # error: [explicit-format-string-type-conversion]

# error: [explicit-format-string-type-conversion]
"{}{}".format(repr(foo), ascii(bar))  # error: [explicit-format-string-type-conversion]

# error: [explicit-format-string-type-conversion]
"%(x)s %(y)s" % {"x": repr(foo), "y": str(bar)}  # error: [explicit-format-string-type-conversion]

# error: [explicit-format-string-type-conversion]
"{0} {1} {0}".format(repr(foo), str(bar))  # error: [explicit-format-string-type-conversion]
```

## Alongside the f-string rules

`printf-string-formatting` (`UP031`) and `f-string` (`UP032`) rewrite the same expressions into
f-strings, which `explicit-f-string-type-conversion` (`RUF010`) then rewrites in turn. Both
diagnostics are reported, but their fixes overlap, so only one of them is applied per pass and
the two paths converge on the same f-string.

```toml
lint.preview = true
lint.select = ["RUF010", "RUF077", "UP031", "UP032"]
```

```py
foo = "foo"

# error: [printf-string-formatting]
"%s" % (repr(foo),)  # error: [explicit-format-string-type-conversion]

# error: [f-string]
"{}".format(repr(foo))  # error: [explicit-format-string-type-conversion]
```

## Generator expressions

A generator expression that goes unparenthesized inside the call has to be parenthesized once
it is hoisted out:

```py
values = [1, 2]

"{} {}".format(repr(value for value in values), 1)  # snapshot: explicit-format-string-type-conversion
"%s" % (repr(value for value in values),)  # error: [explicit-format-string-type-conversion]
```

```snapshot
error[RUF077]: Use the `!r` conversion flag instead of calling `repr()`
 --> src/mdtest_snippet.py:3:16
  |
3 | "{} {}".format(repr(value for value in values), 1)  # snapshot: explicit-format-string-type-conversion
  |                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
  |
help: Replace with `!r` conversion flag
  |
2 |
  - "{} {}".format(repr(value for value in values), 1)  # snapshot: explicit-format-string-type-conversion
3 + "{!r} {}".format((value for value in values), 1)  # snapshot: explicit-format-string-type-conversion
4 | "%s" % (repr(value for value in values),)  # error: [explicit-format-string-type-conversion]
  |
```
