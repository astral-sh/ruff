this_should_raise_Q004 = 'This is a \"string\"'
this_should_raise_Q004 = 'This is \\ a \\\"string\"'
this_is_fine = '"This" is a \"string\"'
this_is_fine = "This is a 'string'"
this_is_fine = "\"This\" is a 'string'"
this_is_fine = r'This is a \"string\"'
this_is_fine = R'This is a \"string\"'
this_should_raise_Q004 = (
    'This is a'
    '\"string\"'
)

# Same as above, but with f-strings
f'This is a \"string\"'  # Q004
f'This is \\ a \\\"string\"'  # Q004
f'"This" is a \"string\"'
f"This is a 'string'"
f"\"This\" is a 'string'"
fr'This is a \"string\"'
fR'This is a \"string\"'
this_should_raise_Q004 = (
    f'This is a'
    f'\"string\"'  # Q004
)

# Nested f-strings (Python 3.12+)
#
# The first one is interesting because the fix for it is valid pre 3.12:
#
#   f"'foo' {'nested'}"
#
# but as the actual string itself is invalid pre 3.12, we don't catch it.
f'\"foo\" {'nested'}'  # Q004
f'\"foo\" {f'nested'}'  # Q004
f'\"foo\" {f'\"nested\"'} \"\"'  # Q004

f'normal {f'nested'} normal'
f'\"normal\" {f'nested'} normal'  # Q004
f'\"normal\" {f'nested'} "double quotes"'
f'\"normal\" {f'\"nested\" {'other'} normal'} "double quotes"'  # Q004
f'\"normal\" {f'\"nested\" {'other'} "double quotes"'} normal'  # Q004

# Make sure we do not unescape quotes
this_is_fine = 'This is an \\"escaped\\" quote'
this_should_raise_Q004 = 'This is an \\\"escaped\\\" quote with an extra backslash'
