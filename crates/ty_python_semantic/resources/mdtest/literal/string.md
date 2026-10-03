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
reveal_type("q\u0301")  # revealed: Literal["q́"]
reveal_type("\u093f")  # revealed: Literal["\u093f"]
reveal_type("\u20dd")  # revealed: Literal["\u20dd"]
reveal_type("\u0301q\u0301")  # revealed: Literal["\u0301q́"]
reveal_type("a\n\u0301\u0302")  # revealed: Literal["a\n\u0301\u0302"]
reveal_type('a"b\x00\ufe20')  # revealed: Literal['a"b\x00\ufe20']
reveal_type("a'\"\u0301")  # revealed: Literal["a'\"\u0301"]
reveal_type("a'\u0301")  # revealed: Literal["a'́"]
reveal_type("\\\u0301")  # revealed: Literal["\\\u0301"]
reveal_type("\U0001d167")  # revealed: Literal["\U0001d167"]
```

## Canonically equivalent strings

Distinct strings can render identically when they are [canonically equivalent]. ty escapes non-ASCII
characters that can participate in normalization in parts of the string that are not in [NFC], so
these strings can be distinguished.

NFC combines `e\u0301` into `é`, whereas `q\u0301` is already in NFC. It also puts combining marks
into a standard order.

```py
from typing import Literal

reveal_type("é")  # revealed: Literal["é"]
reveal_type("e\u0301")  # revealed: Literal["e\u0301"]
reveal_type("q\u0301 e\u0301")  # revealed: Literal["q́ e\u0301"]
reveal_type("é e\u0301")  # revealed: Literal["é e\u0301"]
reveal_type("Å")  # revealed: Literal["Å"]
reveal_type("\u212b")  # revealed: Literal["\u212b"]
reveal_type("가")  # revealed: Literal["가"]
reveal_type("\u1100\u1161")  # revealed: Literal["\u1100\u1161"]
reveal_type("q\u0315\u0300")  # revealed: Literal["q\u0315\u0300"]
reveal_type("q\u0300\u0315")  # revealed: Literal["q̀̕"]

def equivalent_literals(value: Literal["é", "e\u0301"]):
    reveal_type(value)  # revealed: Literal["é", "e\u0301"]
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

[canonically equivalent]: https://www.unicode.org/reports/tr15/#Canon_Compat_Equivalence
[combining marks]: https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-7/#G18130
[default-ignorable characters]: https://www.unicode.org/reports/tr44/#Default_Ignorable_Code_Point
[nfc]: https://www.unicode.org/reports/tr15/#Norm_Forms
