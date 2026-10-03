# String literals

## Simple

```py
reveal_type("Hello")  # revealed: Literal["Hello"]
reveal_type("world")  # revealed: Literal["world"]
reveal_type("Guten " + "Tag")  # revealed: Literal["Guten Tag"]
reveal_type("bon " + "jour")  # revealed: Literal["bon jour"]
```

## Nested Quotes

```py
reveal_type('I say "hello" to you')  # revealed: Literal['I say "hello" to you']

# revealed: Literal['You say "hey" back']
reveal_type("You say \"hey\" back")  # fmt: skip

reveal_type('No "closure here')  # revealed: Literal['No "closure here']
reveal_type("a'\"b")  # revealed: Literal["a'\"b"]
```

## Escaped characters

The escaping of non-printable characters matches Python's `repr`. ty prefers double quotes, so the
surrounding quotes may differ.

```py
reveal_type("\x1b\u200b\U000e0001")  # revealed: Literal["\x1b\u200b\U000e0001"]
```

## Combining marks

When displaying a string literal, ty escapes [combining marks] at the start of the string and after
characters displayed as escape sequences. This prevents the marks from attaching to the opening
quote or to the escape sequence.

```py
reveal_type("\ufe20")  # revealed: Literal["\ufe20"]
reveal_type("\u0301")  # revealed: Literal["\u0301"]
reveal_type("e\u0301")  # revealed: Literal["é"]
reveal_type("\u093f")  # revealed: Literal["\u093f"]
reveal_type("\u20dd")  # revealed: Literal["\u20dd"]
reveal_type("\u0301e\u0301")  # revealed: Literal["\u0301é"]
reveal_type("a\n\u0301\u0302")  # revealed: Literal["a\n\u0301\u0302"]
reveal_type('a"b\x00\ufe20')  # revealed: Literal['a"b\x00\ufe20']
reveal_type("a'\"\u0301")  # revealed: Literal["a'\"\u0301"]
reveal_type("a'\u0301")  # revealed: Literal["a'́"]
reveal_type("\\\u0301")  # revealed: Literal["\\\u0301"]
reveal_type("\U0001d167")  # revealed: Literal["\U0001d167"]
```

## Default-ignorable characters

[Default-ignorable characters] can be invisible when rendered, so ty escapes them to distinguish
literals that differ only by such a character.

```py
from typing import Literal

reveal_type("a\u034f")  # revealed: Literal["a\u034f"]
reveal_type("a\u115f")  # revealed: Literal["a\u115f"]
reveal_type("a\ufe0f")  # revealed: Literal["a\ufe0f"]
reveal_type("a\U000e0100")  # revealed: Literal["a\U000e0100"]
reveal_type("a\u200b")  # revealed: Literal["a\u200b"]

def variants(value: Literal["a", "a\u034f"]):
    reveal_type(value)  # revealed: Literal["a", "a\u034f"]
```

A combining mark following an escaped default-ignorable character is also escaped, preventing it
from attaching to the escape sequence:

```py
reveal_type("a\u034f\u0301")  # revealed: Literal["a\u034f\u0301"]
```

[combining marks]: https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-7/#G18130
[default-ignorable characters]: https://www.unicode.org/reports/tr44/#Default_Ignorable_Code_Point
