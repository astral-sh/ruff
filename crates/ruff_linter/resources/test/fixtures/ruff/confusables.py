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

# Same test cases as above but using f-strings instead:
x = f"𝐁ad string"
x = f"−"
x = f"Русский"
x = f"βα Bαd"
x = f"Р усский"

# Nested f-strings
x = f"𝐁ad string {f" {f"Р усский"}"}"

# Comments inside f-strings
x = f"string { # And here's a comment with an unusual parenthesis: ）
# And here's a comment with a greek alpha: ∗
foo # And here's a comment with an unusual punctuation mark: ᜵
}"

# At runtime the attribute will be stored as Greek small letter mu instead of
# micro sign because of PEP 3131's NFKC normalization
class Labware:
    µL = 1.5


assert getattr(Labware(), "µL") == 1.5

# Implicit string concatenation
x = "𝐁ad" f"𝐁ad string"
