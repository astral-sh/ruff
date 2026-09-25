# Regular expressions

## Required groups

A successful match of a literal pattern returns a string for every group that participates in all
matches. The match object still has the usual `re.Match` type.

```py
import re

def check(text: str):
    match = re.search(r"name: (\w+)", text)
    reveal_type(match)  # revealed: Match[str] | None
    assert match is not None
    reveal_type(match)  # revealed: Match[str]
    reveal_type(match.group())  # revealed: str
    reveal_type(match.group(0))  # revealed: str
    reveal_type(match.group(1))  # revealed: str
    reveal_type(match[0])  # revealed: str
    reveal_type(match[1])  # revealed: str

    match = re.match(r"(\w+)", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

    match = re.fullmatch(r"(\w+)", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str
```

## Optional and named groups

A group that can be skipped may return `None`. Names and numeric indices identify the same groups.

```py
import re

def check(text: str):
    match = re.fullmatch(r"(?P<name>\w+)(?::(?P<value>\d+))?", text)
    assert match is not None
    reveal_type(match.group("name"))  # revealed: str
    reveal_type(match.group("value"))  # revealed: str | None
    reveal_type(match.group(1))  # revealed: str
    reveal_type(match.group(2))  # revealed: str | None
    reveal_type(match["name"])  # revealed: str
    reveal_type(match["value"])  # revealed: str | None
    reveal_type(match[2])  # revealed: str | None
```

## Multiple groups and defaults

Each element of a tuple returned by `group` or `groups` reflects the corresponding group's
participation. A default replaces `None` only for groups that can be skipped. The values of
`groupdict` include only named groups.

```py
import re

class Default: ...

def check(text: str, default: Default, indices: tuple[str | int, ...]):
    match = re.fullmatch(r"(?P<name>\w+)(?::(?P<value>\d+))?", text)
    assert match is not None
    reveal_type(match.group(0, 2, "name"))  # revealed: tuple[str, str | None, str]
    reveal_type(match.group(*(0, 2, "name")))  # revealed: tuple[str, str | None, str]
    reveal_type(match.group(0, *(2, "name")))  # revealed: tuple[str, str | None, str]
    reveal_type(match.group(*(1,)))  # revealed: str
    reveal_type(match.group(*()))  # revealed: str
    reveal_type(match.group(1, 2, *indices))  # revealed: tuple[str | None, ...]
    reveal_type(match.groups())  # revealed: tuple[str, str | None]
    reveal_type(match.groups(default))  # revealed: tuple[str, str | Default]
    reveal_type(match.groups(default=default))  # revealed: tuple[str, str | Default]
    reveal_type(match.groupdict())  # revealed: dict[str, str | None]
    reveal_type(match.groupdict(default))  # revealed: dict[str, str | Default]

    match = re.fullmatch(r"(?P<name>\w+)(:\d+)?", text)
    assert match is not None
    reveal_type(match.groupdict())  # revealed: dict[str, str]
    reveal_type(match.groupdict(default=default))  # revealed: dict[str, str]
```

## Bytes patterns

The same group information is available for bytes patterns, with `bytes` in place of `str`.

```py
import re

def check(text: bytes):
    match = re.search(rb"(?P<name>\w+)(?::(\d+))?", text)
    reveal_type(match)  # revealed: Match[bytes] | None
    assert match is not None
    reveal_type(match.group())  # revealed: bytes
    reveal_type(match.group("name"))  # revealed: bytes
    reveal_type(match.group(2))  # revealed: bytes | None
    reveal_type(match["name"])  # revealed: bytes
    reveal_type(match[2])  # revealed: bytes | None
    reveal_type(match.group(1, 2))  # revealed: tuple[bytes, bytes | None]
    reveal_type(match.groups())  # revealed: tuple[bytes, bytes | None]
    reveal_type(match.groupdict())  # revealed: dict[str, bytes]
```

## Compiled patterns

Compiling a literal pattern preserves its group information for subsequent matching operations.

```py
import re

def check(text: str):
    pattern = re.compile(r"(?P<name>\w+)(:\d+)?")
    reveal_type(pattern)  # revealed: Pattern[str]

    match = pattern.search(text)
    assert match is not None
    reveal_type(match.group("name"))  # revealed: str
    reveal_type(match.group(2))  # revealed: str | None

    match = pattern.match(text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

    match = pattern.fullmatch(text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

    for match in pattern.finditer(text):
        reveal_type(match)  # revealed: Match[str]
        reveal_type(match.groups())  # revealed: tuple[str, str | None]

def check_bytes(text: bytes):
    pattern = re.compile(rb"(\w+)(:\d+)?")
    reveal_type(pattern)  # revealed: Pattern[bytes]
    match = pattern.search(text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[bytes, bytes | None]
```

## Reusing compiled patterns

Passing a compiled pattern through `compile`, or directly to a module-level matching function,
preserves its group information.

```py
import re

def check(text: str):
    pattern = re.compile(r"(a)(b)?")
    copied = re.compile(pattern)
    match = copied.search(text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str | None]

    match = re.search(pattern, text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str | None]

    next_match = match.re.search(text)
    assert next_match is not None
    reveal_type(next_match.groups())  # revealed: tuple[str, str | None]
```

## Unions of compiled patterns

When a group is optional in one possible pattern, its value may be `None`.

```py
import re

def check(text: str, condition: bool):
    pattern = re.compile(r"(a)") if condition else re.compile(r"(a)?")
    match = pattern.search(text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str | None
```

## Bound method aliases

Storing a pattern's matching method or a match's group method in a variable preserves the group
information associated with that method's receiver.

```py
import re

def check(text: str):
    search = re.compile(r"(a)(b)?").search
    match = search(text)
    assert match is not None
    group = match.group
    reveal_type(group(1))  # revealed: str
    reveal_type(group(2))  # revealed: str | None
```

## Compatibility with ordinary pattern and match types

Group information refines the usual pattern and match types. It does not identify a single runtime
object or give that object's string representation a literal type.

```py
import re
from ty_extensions import static_assert
from ty_extensions._internal import TypeOf, is_singleton, is_subtype_of

def check(text: str):
    pattern = re.compile(r"(a)")
    static_assert(is_subtype_of(TypeOf[pattern], re.Pattern[str]))
    static_assert(not is_singleton(TypeOf[pattern]))
    annotated_pattern: re.Pattern[str] = pattern
    reveal_type(str(pattern))  # revealed: str
    reveal_type(repr(pattern))  # revealed: str

    match = pattern.search(text)
    assert match is not None
    static_assert(is_subtype_of(TypeOf[match], re.Match[str]))
    static_assert(not is_singleton(TypeOf[match]))
    annotated_match: re.Match[str] = match
    reveal_type(str(match))  # revealed: str
    reveal_type(repr(match))  # revealed: str
```

## Mutable collections

Inferred mutable collections use the ordinary pattern and match types, so they can also hold objects
with different capture groups. An empty group dictionary retains its general value type.

```py
import re

def check(text: str):
    patterns = [re.compile(r"(a)")]
    patterns.append(re.compile(r"(b)?"))
    reveal_type(patterns)  # revealed: list[Pattern[str]]

    first = re.search(r"(a)", text)
    second = re.search(r"(b)?", text)
    assert first is not None and second is not None
    matches = [first]
    matches.append(second)
    reveal_type(matches)  # revealed: list[Match[str]]

    groups = first.groupdict()
    groups["new"] = "value"
    reveal_type(groups)  # revealed: dict[str, str | None]
```

## Iterating over matches

Every match produced by `finditer` has the literal pattern's group information.

```py
import re

def check(text: str):
    for match in re.finditer(r"(\w+)(:\d+)?", text):
        reveal_type(match)  # revealed: Match[str]
        reveal_type(match.group(1))  # revealed: str
        reveal_type(match.group(2))  # revealed: str | None

def check_bytes(text: bytes):
    for match in re.finditer(rb"(\w+)(:\d+)?", text):
        reveal_type(match.group(1))  # revealed: bytes
        reveal_type(match.group(2))  # revealed: bytes | None
```

## Alternation and repetition

A capture in only one branch of an alternation is optional. A capture surrounding the whole
alternation still participates in every successful match.

```py
import re

def check(text: str):
    match = re.fullmatch(r"((a)|(b))", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str | None, str | None]
```

Repetition with a zero lower bound can skip its captures. Repeating a capture at least once
preserves its required status.

```py
def repeated(text: str):
    match = re.fullmatch(r"(a)?(b)*(c)+(d){0,2}(e){2,3}", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str | None, str | None, str, str | None, str]

    match = re.fullmatch(r"(a(b)?)?", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str | None, str | None]
```

## Literal braces

Braces that do not form a repetition count match literal characters. They do not make an adjacent
capture optional or hide a capture written between them.

```py
import re

def check(text: str):
    match = re.fullmatch(r"(a){foo}", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

    match = re.fullmatch(r"{(a)}", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str]
```

Even in verbose mode, whitespace inside braces prevents them from forming a repetition count.

```py
def verbose(text: str):
    match = re.fullmatch(r"(?x)(a){ 0,1 }", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str
```

## Noncapturing groups, escapes, and character classes

Noncapturing groups, escaped parentheses, and parentheses inside character classes do not create
additional captures.

```py
import re

def check(text: str):
    match = re.fullmatch(r"(?:\(([^()]+)\))[()]", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str]
```

## Octal escapes

An escape with three octal digits represents a character, rather than a backreference. Escaped
parentheses inside character classes do not create capturing groups.

```py
import re

def check(text: str):
    match = re.fullmatch(r"(a)\141", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

def check_bytes(text: bytes):
    match = re.fullmatch(rb"(\141)[\050-\051](b)?", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[bytes, bytes | None]

    match = re.fullmatch(rb"([\200-\377])", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: bytes
```

## Unicode capture names

Capture names in string patterns accept Python identifiers, including `℘` and combining characters
after the first character.

```py
import re

def check(text: str):
    match = re.fullmatch("(?P<℘>a)(?P<a\u0301>b)", text)
    assert match is not None
    reveal_type(match.group("℘"))  # revealed: str
    reveal_type(match.group("a\u0301"))  # revealed: str
    reveal_type(match.groups())  # revealed: tuple[str, str]
```

## Lookarounds and inline flags

A capture in a successful positive lookaround participates in the match. A capture in a negative
lookaround is not required. Inline flags do not create additional groups.

```py
import re

def check(text: str):
    match = re.search(r"(?i)(?=(a))(?P<name>a)(?!(b))", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str
    reveal_type(match.group("name"))  # revealed: str
    reveal_type(match.group(3))  # revealed: str | None

    match = re.search(r"(?<=(a))(?i:b)(c)", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str]
```

## Explicit flags and keyword arguments

Known flags preserve group information. Verbose mode ignores captures written inside comments,
whether selected by an enum member or its numeric value. Keyword arguments have the same behavior as
positional arguments.

```py
import re

def check(text: str):
    match = re.search(r"(a)", text, re.I)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

    match = re.search("(a) # (ignored)\n (b)?", text, re.X)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str | None]

    match = re.search("(a) # (ignored)", text, 64)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str]

    match = re.search(pattern=r"(a)", string=text, flags=0)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str

    pattern = re.compile(pattern=r"(a)", flags=re.I)
    match = pattern.search(string=text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str
```

## Late global flags in Python 3.10

Python 3.10 permits global inline flags after other parts of the pattern. Verbose mode then applies
to the whole pattern: `(fake)` is a comment, and the second capture is the optional `(b)`.

```toml
[environment]
python-version = "3.10"
```

```py
import re

def check(text: str):
    match = re.search("(a) # (fake)\n(b)?(?x)", text)
    assert match is not None
    reveal_type(match.group(2))  # revealed: str | None
```

## Dynamic patterns and group indices

Without a known pattern, group methods retain their general signatures. Unknown group indices may
refer to groups that do not participate in every match.

```py
import re

def dynamic(pattern: str, text: str):
    match = re.search(pattern, text)
    assert match is not None
    reveal_type(match.group())  # revealed: str
    reveal_type(match.group(1))  # revealed: str | None
    reveal_type(match[1])  # revealed: str | None
    reveal_type(match.groups())  # revealed: tuple[str | None, ...]
    reveal_type(match.groupdict())  # revealed: dict[str, str | None]

def unknown_index(text: str, group: str | int):
    match = re.search(r"(a)(b)?", text)
    assert match is not None
    reveal_type(match.group(group))  # revealed: str | None
    reveal_type(match[group])  # revealed: str | None

def annotated(match: re.Match[bytes]):
    reveal_type(match.group(1))  # revealed: bytes | None
    reveal_type(match.groups())  # revealed: tuple[bytes | None, ...]
```

## Unsupported patterns and unknown flags

Backreferences, conditionals, and flags whose values are unknown retain the general match type.

```py
import re

def check(text: str, flags: int):
    match = re.search(r"(a)\1", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str | None

    match = re.search(r"(a)?(?(1)b|c)", text)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str | None

    match = re.search(r"(a)", text, flags)
    assert match is not None
    reveal_type(match.group(1))  # revealed: str | None
```

## Returning captured groups

Required captures can be returned as strings even with `unsound-return-statement` enabled. Optional
captures still require handling `None`.

Regression test for <https://github.com/astral-sh/ty/issues/4578>.

```toml
[rules]
unsound-return-statement = "error"
```

```py
import re

def required(text: str) -> str:
    match = re.fullmatch(r"name: (\w+)", text)
    assert match is not None
    return match.group(1)

def optional(text: str) -> str:
    match = re.fullmatch(r"name: (\w+)?", text)
    assert match is not None
    return match.group(1)  # error: [invalid-return-type]
```

## Prefix matching in Python 3.15

`prefixmatch` preserves the same capture information as `match`.

```toml
[environment]
python-version = "3.15"
```

```py
import re

def check(text: str):
    match = re.prefixmatch(r"(a)(b)?", text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str | None]

    match = re.compile(r"(a)(b)?").prefixmatch(text)
    assert match is not None
    reveal_type(match.groups())  # revealed: tuple[str, str | None]
```
