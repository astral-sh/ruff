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

Distinct strings can render identically when they are [canonically equivalent]. ty may escape
non-ASCII characters in parts of the string that are not in [NFC], so these strings can be
distinguished. It uses ASCII and some NFC-inert characters as boundaries when checking these parts.
NFC-inert characters do not interact with adjacent characters during normalization. ty may also
escape other characters that do not participate in normalization.

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

ty recognizes `☃`, `字`, and `𠀀` as NFC-inert but does not recognize `⏱`. In the first example,
`é⏱\u212b` is checked as one part and all three characters are escaped. In the others, the inert
character separates `é` from `\u212b`, so `é` remains unescaped:

```py
reveal_type("é⏱\u212b")  # revealed: Literal["\xe9\u23f1\u212b"]
reveal_type("é☃\u212b")  # revealed: Literal["é☃\u212b"]
reveal_type("é字\u212b")  # revealed: Literal["é字\u212b"]
reveal_type("é𠀀\u212b")  # revealed: Literal["é𠀀\u212b"]
```

ASCII characters provide a boundary before themselves, even if they can combine with following
characters. In `ée\u0301`, the `e` starts a non-NFC segment, so ty escapes the combining accent
without escaping the preceding `é`:

```py
reveal_type("ée\u0301")  # revealed: Literal["ée\u0301"]
```

The following two strings can look identical, but the first contains the single character `é`, while
the second contains `e` followed by a separate accent:

```py
reveal_type("q\u0301é")  # revealed: Literal["q́é"]
```

In `q\u0301e\u0301`, the boundary before `e` lets ty check the two parts separately. `q\u0301` is
already in NFC, and there is no equivalent single character for `q` plus this accent, so the accent
remains visible. By contrast, `e\u0301` is canonically equivalent to the single character `é`, so ty
escapes its accent to distinguish the type from the preceding example:

```py
reveal_type("q\u0301e\u0301")  # revealed: Literal["q́e\u0301"]
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
