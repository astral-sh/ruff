# PLE1310
"Hello World".strip("Hello")

# PLE1310
"Hello World".strip("Hello")

# PLE1310
"Hello World".strip(u"Hello")

# PLE1310
"Hello World".strip(r"Hello")

# PLE1310
"Hello World".strip("Hello\t")

# PLE1310
"Hello World".strip(r"Hello\t")

# PLE1310
"Hello World".strip("Hello\\")

# PLE1310
"Hello World".strip(r"Hello\\")

# PLE1310
"Hello World".strip("🤣🤣🤣🤣🙃👀😀")

# PLE1310
"Hello World".strip(
    """
there are a lot of characters to strip
"""
)

# PLE1310
"Hello World".strip("can we get a long " \
                    "string of characters to strip " \
                    "please?")

# PLE1310
"Hello World".strip(
    "can we get a long "
    "string of characters to strip "
    "please?"
)

# PLE1310
"Hello World".strip(
    "can \t we get a long"
    "string \t of characters to strip"
    "please?"
)

# PLE1310
"Hello World".strip(
    "abc def"
    "ghi"
)

# PLE1310
u''.strip('http://')

# PLE1310
u''.lstrip('http://')

# PLE1310
b''.rstrip(b'http://')

# OK
''.strip('Hi')

# OK
''.strip()


### https://github.com/astral-sh/ruff/issues/15968

# Errors: Multiple backslashes
''.strip('\\b\\x09')
''.strip(r'\b\x09')
''.strip('\\\x5C')

# Errors: Type inference
b = b''
b.strip(b'//')

# Errors: Type inference (preview)
foo: str = ""; bar: bytes = b""
foo.rstrip("//")
bar.lstrip(b"//")


# OK: Different types
b"".strip("//")
"".strip(b"//")

# OK: Escapes
'\\test'.strip('\\')

# OK: Extra/missing arguments
"".strip("//", foo)
b"".lstrip(b"//", foo = "bar")
"".rstrip()

# OK: Not literals
"".strip(foo)
b"".strip(bar)

# OK: Not `.[lr]?strip`
"".mobius_strip("")
