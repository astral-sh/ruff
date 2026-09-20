import re

b_src = b"abc"

# Should be replaced with `b_src.replace(rb"x", b"y")`
re.sub(rb"x", b"y", b_src)

# Should be replaced with `b_src.startswith(rb"abc")`
if re.match(rb"abc", b_src):
    pass

# Should be replaced with `rb"x" in b_src`
if re.search(rb"x", b_src):
    pass

# Should be replaced with `b_src.split(rb"abc")`
re.split(rb"abc", b_src)

# Patterns containing metacharacters should NOT be replaced
re.sub(rb"ab[c]", b"", b_src)
re.match(rb"ab[c]", b_src)
re.search(rb"ab[c]", b_src)
re.fullmatch(rb"ab[c]", b_src)
re.split(rb"ab[c]", b_src)

# Empty pattern: re.split(rb"", b_src) should not be flagged
re.split(rb"", b_src)

# A buffer-protocol target is not a `bytes`. `re.search` searches any buffer,
# but `in` only searches a real `bytes`, so the fix must be unsafe.
# https://github.com/astral-sh/ruff/issues/27024
mv = memoryview(b"abc")
assert re.search(b"ab", mv)
assert re.search(b"ab", memoryview(b"abc"))

# A `bytearray` target is also not a `bytes` for the purposes of `in`.
ba = bytearray(b"abc")
assert re.search(b"ab", ba)

# A `bytes`-literal target is still safely fixable.
assert re.search(b"ab", b"abc")
