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

# Block conversion if there is an unescaped quote just before the end of the triple
# quoted string
r'''\""'''
r'''""'''
r'\""'

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
    "a" # trailing part comment

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


# Regression test for https://github.com/astral-sh/ruff/issues/5893
x = ("""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""" """bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb""")
x = (f"""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""" f"""bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb""")
x = (b"""aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa""" b"""bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb""")

# https://github.com/astral-sh/ruff/issues/7460
trailing_preferred_quote_texts = [''' "''', ''' ""''', ''' """''', ''' """"''']

a = f"""\x1F"""
a = """\x1F"""
a = """\\x1F"""
a = """\\\x1F"""


##############################################################################
# Implicit concatenated strings
##############################################################################

# In preview, don't collapse implicit concatenated strings that can't be joined into a single string
# and that are multiline in the source.

(
    r"aaaaaaaaa"
    r"bbbbbbbbbbbbbbbbbbbb"
)

(
    r"aaaaaaaaa" r"bbbbbbbbbbbbbbbbbbbb"
    "cccccccccccccccccccccc"
)

(
    """aaaaaaaaa"""
    """bbbbbbbbbbbbbbbbbbbb"""
)

(
    f"""aaaa{
    10}aaaaa"""
    fr"""bbbbbbbbbbbbbbbbbbbb"""
)

if (
       r"aaaaaaaaa"
       r"bbbbbbbbbbbbbbbbbbbb"
   ) + ["aaaaaaaaaa", "bbbbbbbbbbbbbbbb"]: ...



# In preview, keep implicit concatenated strings on a single line if they fit and are not separate by line breaks in the source
(
    r"aaaaaaaaa" r"bbbbbbbbbbbbbbbbbbbb"
)

r"aaaaaaaaa" r"bbbbbbbbbbbbbbbbbbbb"

(
    f"aaaa{
    10}aaaaa" fr"bbbbbbbbbbbbbbbbbbbb"
)

(
    r"""aaaaaaaaa""" r"""bbbbbbbbbbbbbbbbbbbb"""
)

(
    f"""aaaa{
    10}aaaaa""" fr"""bbbbbbbbbbbbbbbbbbbb"""
)

# In docstring positions
def docstring():
    (
        r"aaaaaaaaa"
        "bbbbbbbbbbbbbbbbbbbb"
    )

def docstring_flat():
    (
        r"aaaaaaaaa" r"bbbbbbbbbbbbbbbbbbbb"
    )

def docstring_flat_overlong():
    (
        r"aaaaaaaaa" r"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    )
