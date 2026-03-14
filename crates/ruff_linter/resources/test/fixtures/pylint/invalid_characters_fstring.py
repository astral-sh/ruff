# Tests for invalid characters inside f-string interpolation expressions.
# In Python < 3.12, backslash escapes are not allowed inside f-string {}.
# The fix should be suppressed when target-version < 3.12.

# Control char in string literal inside f-string interpolation
a = f"{'helloworld'}"

# Control char in format spec (FStringMiddle inside interpolation)
a = f"{42:>10}"

# Control char in f-string literal part (not interpolation) - fix should always apply
a = f"helloworld"

# Control char in regular string - fix should always apply
a = "helloworld"
