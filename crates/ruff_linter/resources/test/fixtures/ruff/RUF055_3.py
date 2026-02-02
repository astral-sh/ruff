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