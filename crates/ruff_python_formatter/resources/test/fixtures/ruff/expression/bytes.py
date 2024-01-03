b"' test"
b'" test'

b"\" test"
b'\' test'

# Prefer single quotes for string with more double quotes
b"' \" \" '' \" \" '"

# Prefer double quotes for string with more single quotes
b'\' " " \'\' " " \''

# Prefer double quotes for string with equal amount of single and double quotes
b'" \' " " \'\''
b"' \" '' \" \""

b"\\' \"\""
b'\\\' ""'


b"Test"
B"Test"

rb"Test"
Rb"Test"

b'This string will not include \
backslashes or newline characters.'

if True:
    b'This string will not include \
        backslashes or newline characters.'

b"""Multiline
String \"
"""

b'''Multiline
String \'
'''

b'''Multiline
String ""
'''

b'''Multiline
String """
'''

b'''Multiline
String "'''

b"""Multiline
String '''
"""

b"""Multiline
String '"""

b'''Multiline
String \"\"\"
'''

# String continuation

b"Let's" b"start" b"with" b"a" b"simple" b"example"

b"Let's" b"start" b"with" b"a" b"simple" b"example" b"now repeat after me:" b"I am confident" b"I am confident" b"I am confident" b"I am confident" b"I am confident"

(
    b"Let's" b"start" b"with" b"a" b"simple" b"example" b"now repeat after me:" b"I am confident" b"I am confident" b"I am confident" b"I am confident" b"I am confident"
)

if (
    a + b"Let's"
        b"start"
        b"with"
        b"a"
        b"simple"
        b"example"
        b"now repeat after me:"
        b"I am confident"
        b"I am confident"
        b"I am confident"
        b"I am confident"
        b"I am confident"
):
    pass

if b"Let's" b"start" b"with" b"a" b"simple" b"example" b"now repeat after me:" b"I am confident" b"I am confident" b"I am confident" b"I am confident" b"I am confident":
    pass

(
    # leading
    b"a" # trailing part comment

    # leading part comment

    b"b" # trailing second part comment
    # trailing
)

test_particular = [
    # squares
    b'1.00000000100000000025',
    b'1.0000000000000000000000000100000000000000000000000' #...
    b'00025',
    b'1.0000000000000000000000000000000000000000000010000' #...
    b'0000000000000000000000000000000000000000025',
]

# Parenthesized string continuation with messed up indentation
{
    "key": (
        [],
    b'a'
        b'b'
    b'c'
    )
}

b"Unicode Escape sequence don't apply to bytes: \N{0x} \u{ABCD} \U{ABCDEFGH}"
