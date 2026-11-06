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

When displaying a string literal, ty escapes [combining marks] that would attach to the opening
quote or to an escape sequence.

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

Unicode's grapheme rules treat the Myanmar vowel sign `ါ` as separate from a preceding quote, so ty
can display it without an escape:

```py
reveal_type("\u102b")  # revealed: Literal["ါ"]
```

## Other characters that attach to quotes

Some characters can attach to a quote even though they are not combining marks. An emoji modifier
and the Thai character `ำ` attach to the preceding character, while Malayalam dot reph `ൎ` attaches
to the following character. ty escapes them next to the corresponding quote and leaves them visible
when they are joined to a character inside the string.

```py
reveal_type("\U0001f3fb")  # revealed: Literal["\U0001f3fb"]
reveal_type("👍🏻")  # revealed: Literal["👍🏻"]
reveal_type("\u0e33")  # revealed: Literal["\u0e33"]
reveal_type("กำ")  # revealed: Literal["กำ"]
reveal_type("\u0d4e")  # revealed: Literal["\u0d4e"]
reveal_type("ൎക")  # revealed: Literal["ൎക"]
```

An escape sequence also has to stay separate from a preceding Malayalam dot reph:

```py
reveal_type("\u0d4e\u200b")  # revealed: Literal["\u0d4e\u200b"]
```

The Angstrom sign `\u212b` normalizes to `Å` in NFC, so ty displays it as an escape to keep the two
values distinct. A dot reph before that escape would attach to it, so ty escapes it too. A preceding
dot reph would then attach to this new escape and is escaped as well:

```py
reveal_type("\u0d4e\u0d4e\u212b")  # revealed: Literal["\u0d4e\u0d4e\u212b"]
```

## Canonically equivalent strings

Distinct strings can render identically when they are [canonically equivalent]. ty escapes
characters that would make the display non-[NFC], so these strings can be distinguished without
changing their values.

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
reveal_type("\u1100\u1161")  # revealed: Literal["ᄀ\u1161"]
reveal_type("q\u0315\u0300")  # revealed: Literal["q̕\u0300"]
reveal_type("q\u0300\u0315")  # revealed: Literal["q̀̕"]

def equivalent_literals(value: Literal["é", "e\u0301"]):
    reveal_type(value)  # revealed: Literal["é", "e\u0301"]
```

Characters elsewhere in the string can remain visible when another character needs an escape:

```py
reveal_type("é⏱\u212b")  # revealed: Literal["é⏱\u212b"]
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

## Braille blank

The braille blank can look like an ordinary space, so ty displays it as an escape:

```py
reveal_type(" ")  # revealed: Literal[" "]
reveal_type("\u2800")  # revealed: Literal["\u2800"]
```

[canonically equivalent]: https://www.unicode.org/reports/tr15/#Canon_Compat_Equivalence
[combining marks]: https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-7/#G18130
[default-ignorable characters]: https://www.unicode.org/reports/tr44/#Default_Ignorable_Code_Point
[nfc]: https://www.unicode.org/reports/tr15/#Norm_Forms
