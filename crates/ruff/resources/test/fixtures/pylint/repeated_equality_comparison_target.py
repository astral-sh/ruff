# Errors.

foo == "a" or foo == "b"

foo != "a" and foo != "b"

foo == "a" or foo == "b" or foo == "c"

foo != "a" and foo != "b" and foo != "c"

foo == a or foo == "b" or foo == 3  # Mixed types.

# False negatives.

# The current implementation doesn't support yoda conditions, but PyLint does!

"a" == foo or "b" == foo or "c" == foo

"a" != foo and "b" != foo and "c" != foo

"a" == foo or foo == "b" or "c" == foo  # Mixed yoda conditions.

# Non-errors.

foo == "a" and foo == "b" and foo == "c"  # `and` mixed with `==`.

foo != "a" or foo != "b" or foo != "c"  # `or` mixed with `!=`.

foo == a or foo == b() or foo == c  # Call expression.

foo != a or foo() != b or foo != c  # Call expression.

foo in {"a", "b", "c"}  # Uses membership test already.

foo not in {"a", "b", "c"}  # Uses membership test already.

foo == "a"  # Single comparison.

foo != "a"  # Single comparison.
