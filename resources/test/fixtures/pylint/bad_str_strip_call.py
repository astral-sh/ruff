# These SHOULD have a warning
"Hello World".strip("Hello")

"Hello World".strip("Hello")

"Hello World".strip("🤣🤣🤣🤣🙃👀😀")


"Hello World".strip(
    """
there are a lot of characters I would like to strip today, including $ and @ and maybe even 9
"""
)

"Hello World".strip("can we get a stupidly long" \
                    "string of characters to strip" \
                    "please?")

u''.strip('http://')  # [bad-str-strip-call]
u''.lstrip('http://')  # [bad-str-strip-call]
b''.rstrip('http://')  # [bad-str-strip-call]


# These should NOT have a warning
''.strip('yo')
''.strip()

