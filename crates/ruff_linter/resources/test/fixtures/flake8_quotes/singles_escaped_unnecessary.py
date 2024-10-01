this_should_raise_Q004 = "This is a \'string\'"
this_should_raise_Q004 = "'This' is a \'string\'"
this_is_fine = 'This is a "string"'
this_is_fine = '\'This\' is a "string"'
this_is_fine = r"This is a \'string\'"
this_is_fine = R"This is a \'string\'"
this_should_raise_Q004 = (
    "This is a"
    "\'string\'"
)

# Same as above, but with f-strings
f"This is a \'string\'"  # Q004
f"'This' is a \'string\'"  # Q004
f'This is a "string"'
f'\'This\' is a "string"'
fr"This is a \'string\'"
fR"This is a \'string\'"
this_should_raise_Q004 = (
    f"This is a"
    f"\'string\'"  # Q004
)

# Nested f-strings (Python 3.12+)
#
# The first one is interesting because the fix for it is valid pre 3.12:
#
#   f'"foo" {"nested"}'
#
# but as the actual string itself is invalid pre 3.12, we don't catch it.
f"\'foo\' {"foo"}"  # Q004
f"\'foo\' {f"foo"}"  # Q004
f"\'foo\' {f"\'foo\'"} \'\'"  # Q004

f"normal {f"nested"} normal"
f"\'normal\' {f"nested"} normal"  # Q004
f"\'normal\' {f"nested"} 'single quotes'"
f"\'normal\' {f"\'nested\' {"other"} normal"} 'single quotes'"  # Q004
f"\'normal\' {f"\'nested\' {"other"} 'single quotes'"} normal"  # Q004

# Make sure we do not unescape quotes
this_is_fine = "This is an \\'escaped\\' quote"
this_should_raise_Q004 = "This is an \\\'escaped\\\' quote with an extra backslash"  # Q004

# Invalid escapes in bytestrings are also triggered:
x = b"\xe7\xeb\x0c\xa1\x1b\x83tN\xce=x\xe9\xbe\x01\xb9\x13B_\xba\xe7\x0c2\xce\'rm\x0e\xcd\xe9.\xf8\xd2"  # Q004
