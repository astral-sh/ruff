x = "𝐁ad string"
y = "−"


def f():
    """Here's a docstring with an unusual parenthesis: ）"""
    # And here's a comment with an unusual punctuation mark: ᜵
    ...


def f():
    """Here's a docstring with a greek rho: ρ"""
    # And here's a comment with a greek alpha: ∗
    ...


x = "𝐁ad string"
x = "−"

# This should be ignored, since it contains an unambiguous unicode character, and no
# ASCII.
x = "Русский"

# The first word should be ignored, while the second should be included, since it
# contains ASCII.
x = "βα Bαd"

# The two characters should be flagged here. The first character is a "word"
# consisting of a single ambiguous character, while the second character is a "word
# boundary" (whitespace) that it itself ambiguous.
x = "Р усский"
