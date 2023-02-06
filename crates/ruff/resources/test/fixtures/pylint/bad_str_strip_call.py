# PLE1310
"Hello World".strip("Hello")

# PLE1310
"Hello World".strip("Hello")

# PLE1310
"Hello World".strip(u"Hello")

# PLE1310
"Hello World".strip(r"Hello")

# PLE1310
"Hello World".strip("Hel\tlo")

# PLE1310
"Hello World".strip(r"He\tllo")

# PLE1310
"Hello World".strip("Hel\\lo")

# PLE1310
"Hello World".strip(r"He\\llo")

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
b''.rstrip('http://')

# OK
''.strip('Hi')

# OK
''.strip()
