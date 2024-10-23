U00A0 = "\u00a0"

# Standard Case
# Expects:
# - single StringLiteral
assert True, print("This print is not intentional.")

# Concatenated string literals
# Expects:
# - single StringLiteral
assert True, print("This print" " is not intentional.")

# Positional arguments, string literals
# Expects:
# - single StringLiteral concatenated with " "
assert True, print("This print", "is not intentional")

# Concatenated string literals combined with Positional arguments
# Expects:
# - single stringliteral concatenated with " " only between `print` and `is`
assert True, print("This " "print", "is not intentional.")

# Positional arguments, string literals with a variable
# Expects:
# - single FString concatenated with " "
assert True, print("This", print.__name__, "is not intentional.")

# Mixed brackets string literals
# Expects:
# - single StringLiteral concatenated with " "
assert True, print("This print", 'is not intentional', """and should be removed""")

# Mixed brackets with other brackets inside
# Expects:
# - single StringLiteral concatenated with " " and escaped brackets
assert True, print("This print", 'is not "intentional"', """and "should" be 'removed'""")

# Positional arguments, string literals with a separator
# Expects:
# - single StringLiteral concatenated with "|"
assert True, print("This print", "is not intentional", sep="|")

# Positional arguments, string literals with None as separator
# Expects:
# - single StringLiteral concatenated with " "
assert True, print("This print", "is not intentional", sep=None)

# Positional arguments, string literals with variable as separator, needs f-string
# Expects:
# - single FString concatenated with "{U00A0}"
assert True, print("This print", "is not intentional", sep=U00A0)

# Unnecessary f-string
# Expects:
# - single StringLiteral
assert True, print(f"This f-string is just a literal.")

# Positional arguments, string literals and f-strings
# Expects:
# - single FString concatenated with " "
assert True, print("This print", f"is not {'intentional':s}")

# Positional arguments, string literals and f-strings with a separator
# Expects:
# - single FString concatenated with "|"
assert True, print("This print", f"is not {'intentional':s}", sep="|")

# A single f-string
# Expects:
# - single FString
assert True, print(f"This print is not {'intentional':s}")

# A single f-string with a redundant separator
# Expects:
# - single FString
assert True, print(f"This print is not {'intentional':s}", sep="|")

# Complex f-string with variable as separator
# Expects:
# - single FString concatenated with "{U00A0}", all placeholders preserved
condition = "True is True"
maintainer = "John Doe"
assert True, print("Unreachable due to", condition, f", ask {maintainer} for advice", sep=U00A0)

# Empty print
# Expects:
# - `msg` entirely removed from assertion
assert True, print()

# Empty print with separator
# Expects:
# - `msg` entirely removed from assertion
assert True, print(sep=" ")

# Custom print function that actually returns a string
# Expects:
# - no violation as the function is not a built-in print
def print(s: str):
    return "This is my assertion error message: " + s

assert True, print("this print shall not be removed.")

import builtins

# Use of `builtins.print`
# Expects:
# - single StringLiteral
assert True, builtins.print("This print should be removed.")
