"' test"
'" test'

"\" test"
'\' test'

# Prefer single quotes for string with more double quotes
"' \" \" '' \" \" '"

# Prefer double quotes for string with more single quotes
'\' " " \'\' " " \''

# Prefer double quotes for string with equal amount of single and double quotes
'" \' " " \'\''
"' \" '' \" \""

"\\' \"\""
'\\\' ""'


u"Test"
U"Test"

r"Test"
R"Test"

'This string will not include \
backslashes or newline characters.'

if True:
    'This string will not include \
        backslashes or newline characters.'

"""Multiline
String \"
"""

'''Multiline
String \'
'''

'''Multiline
String ""
'''

'''Multiline
String """
'''

'''Multiline
String "'''

"""Multiline
String '''
"""

"""Multiline
String '"""

'''Multiline
String \"\"\"
'''

# String continuation

"Let's" "start" "with" "a" "simple" "example"

"Let's" "start" "with" "a" "simple" "example" "now repeat after me:" "I am confident" "I am confident" "I am confident" "I am confident" "I am confident"

(
    "Let's" "start" "with" "a" "simple" "example" "now repeat after me:" "I am confident" "I am confident" "I am confident" "I am confident" "I am confident"
)

if (
    a + "Let's"
        "start"
        "with"
        "a"
        "simple"
        "example"
        "now repeat after me:"
        "I am confident"
        "I am confident"
        "I am confident"
        "I am confident"
        "I am confident"
):
    pass

if "Let's" "start" "with" "a" "simple" "example" "now repeat after me:" "I am confident" "I am confident" "I am confident" "I am confident" "I am confident":
    pass

(
    # leading
    "a" # trailing part commen

    # leading part comment

    "b" # trailing second part comment
    # trailing
)

test_particular = [
    # squares
    '1.00000000100000000025',
    '1.0000000000000000000000000100000000000000000000000' #...
    '00025',
    '1.0000000000000000000000000000000000000000000010000' #...
    '0000000000000000000000000000000000000000025',
]

# Parenthesized string continuation with messed up indentation
{
    "key": (
        [],
    'a'
        'b'
    'c'
    )
}
