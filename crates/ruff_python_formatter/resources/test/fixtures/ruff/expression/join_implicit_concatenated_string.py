"aaaaaaaaa" "bbbbbbbbbbbbbbbbbbbb" # Join

(
    "aaaaaaaaaaa" "bbbbbbbbbbbbbbbb"
) # join


(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
) # too long to join


"different '" 'quote "are fine"' # join

# More single quotes
"one single'" "two 'single'" ' two "double"'

# More double quotes
'one double"' 'two "double"' " two 'single'"

# Equal number of single and double quotes
'two "double"' " two 'single'"

f"{'Hy \"User\"'}" 'more'

b"aaaaaaaaa" b"bbbbbbbbbbbbbbbbbbbb" # Join

(
    b"aaaaaaaaaaa" b"bbbbbbbbbbbbbbbb"
) # join


(
    b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
) # too long to join


# Skip joining if there is a trailing comment
(
    "fffffffffffff"
    "bbbbbbbbbbbbb" # comment
    "cccccccccccccc"
)

# Skip joining if there is a leading comment
(
    "fffffffffffff"
    # comment
    "bbbbbbbbbbbbb"
    "cccccccccccccc"
)


##############################################################################
# F-strings
##############################################################################

# Escape `{` and `}` when merging an f-string with a string
"a {not_a_variable}" f"b {10}" "c"

# Join, and break expressions
f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa{
expression
}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" f"cccccccccccccccccccc {20999}" "more"

# Join, but don't break the expressions
f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa{expression}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" f"cccccccccccccccccccc {20999}" "more"

f"test{
expression
}flat" f"can be {
joined
} together"

aaaaaaaaaaa = f"test{
expression
}flat" f"cean beeeeeeee {
joined
} eeeeeeeeeeeeeeeeeeeeeeeeeeeee" # inline


f"single quoted '{x}'" f'double quoted "{x}"' # Same number of quotes => use preferred quote style
f"single quote ' {x}" f'double quoted "{x}"'  # More double quotes => use single quotes
f"single quoted '{x}'" f'double quote " {x}"'  # More single quotes => use double quotes

# Different triple quoted strings
f"{'''test'''}" f'{"""other"""}'

# Now with inner quotes
f"{'''test ' '''}" f'{"""other " """}'
f"{some_where_nested('''test ' ''')}" f'{"""other " """ + "more"}'
f"{b'''test ' '''}" f'{b"""other " """}'
f"{f'''test ' '''}" f'{f"""other " """}'

# debug expressions containing quotes
f"{10 + len('bar')=}" f"{10 + len('bar')=}"
f"{10 + len('bar')=}" f'no debug{10}' f"{10 + len('bar')=}"

# We can't safely merge this pre Python 3.12 without altering the debug expression.
f"{10 + len('bar')=}" f'{10 + len("bar")=}'


##############################################################################
# Don't join raw strings
##############################################################################

r"a" "normal"
R"a" "normal"

f"test" fr"test"
f"test" fR"test"


##############################################################################
# Don't join triple quoted strings
##############################################################################

"single" """triple"""

"single" f""""single"""

b"single" b"""triple"""


##############################################################################
# Join strings in with statements
##############################################################################

# Fits
with "aa" "bbb" "cccccccccccccccccccccccccccccccccccccccccccccc":
    pass

# Parenthesize single-line
with "aa" "bbb" "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc":
    pass

# Multiline
with "aa" "bbb" "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc":
    pass

with f"aaaaaaa{expression}bbbb" f"ccc {20999}" "more":
    pass


##############################################################################
# For loops
##############################################################################

# Flat
for a in "aaaaaaaaa" "bbbbbbbbb" "ccccccccc" "dddddddddd":
    pass

# Parenthesize single-line
for a in "aaaaaaaaa" "bbbbbbbbb" "ccccccccc" "dddddddddd" "eeeeeeeeeeeeeee" "fffffffffffff" "ggggggggggggggg" "hh":
    pass

# Multiline
for a in "aaaaaaaaa" "bbbbbbbbb" "ccccccccc" "dddddddddd" "eeeeeeeeeeeeeee" "fffffffffffff" "ggggggggggggggg" "hhhh":
    pass

##############################################################################
# Assert statement
##############################################################################

# Fits
assert "aaaaaaaaa" "bbbbbbbbbbbb", "cccccccccccccccc" "dddddddddddddddd"

# Wrap right
assert "aaaaaaaaa" "bbbbbbbbbbbb", "cccccccccccccccc" "dddddddddddddddd" "eeeeeeeeeeeee" "fffffffffff"

# Right multiline
assert "aaaaaaaaa" "bbbbbbbbbbbb", "cccccccccccccccc" "dddddddddddddddd" "eeeeeeeeeeeee" "fffffffffffffff" "ggggggggggggg" "hhhhhhhhhhh"

# Wrap left
assert "aaaaaaaaa" "bbbbbbbbbbbb" "cccccccccccccccc" "dddddddddddddddd" "eeeeeeeeeeeee" "fffffffffffffff", "ggggggggggggg" "hhhhhhhhhhh"

# Left multiline
assert "aaaaaaaaa" "bbbbbbbbbbbb" "cccccccccccccccc" "dddddddddddddddd" "eeeeeeeeeeeee" "fffffffffffffff" "ggggggggggggg", "hhhhhhhhhhh"

# wrap both
assert "aaaaaaaaa" "bbbbbbbbbbbb" "cccccccccccccccc" "dddddddddddddddd" "eeeeeeeeeeeee" "fffffffffffffff", "ggggggggggggg" "hhhhhhhhhhh" "iiiiiiiiiiiiiiiiii" "jjjjjjjjjjjjj" "kkkkkkkkkkkkkkkkk" "llllllllllll"

# both multiline
assert "aaaaaaaaa" "bbbbbbbbbbbb" "cccccccccccccccc" "dddddddddddddddd" "eeeeeeeeeeeee" "fffffffffffffff" "ggggggggggggg", "hhhhhhhhhhh" "iiiiiiiiiiiiiiiiii" "jjjjjjjjjjjjj" "kkkkkkkkkkkkkkkkk" "llllllllllll" "mmmmmmmmmmmmmm"


##############################################################################
# In clause headers (can_omit_optional_parentheses)
##############################################################################
# Use can_omit_optional_parentheses layout to avoid an instability where the formatter
# picks the can_omit_optional_parentheses layout when the strings are joined.
if (
    f"implicit"
    "concatenated"
    "string" + f"implicit"
               "concaddddddddddded"
               "ring"
    * len([aaaaaa, bbbbbbbbbbbbbbbb, cccccccccccccccccc, ddddddddddddddddddddddddddd])
):
    pass

# Keep parenthesizing multiline - implicit concatenated strings
if (
    f"implicit"
    """concatenate
    d"""
    "string" + f"implicit"
               "concaddddddddddded"
               "ring"
    * len([aaaaaa, bbbbbbbbbbbbbbbb, cccccccccccccccccc, ddddddddddddddddddddddddddd])
):
    pass

if (
    [
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ]
    + "implicitconcat"
      "enatedstriiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiing"
):
    pass


# In match statements
match x:
    case "implicitconcat" "enatedstring" | [
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ]:
        pass

    case [
            aaaaaa,
            bbbbbbbbbbbbbbbb,
            cccccccccccccccccc,
            ddddddddddddddddddddddddddd,
        ] | "implicitconcat" "enatedstring" :
        pass

    case "implicitconcat" "enatedstriiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiiing" | [
        aaaaaa,
        bbbbbbbbbbbbbbbb,
        cccccccccccccccccc,
        ddddddddddddddddddddddddddd,
    ]:
        pass


##############################################################################
# In docstring positions
##############################################################################

def short_docstring():
    "Implicit" "concatenated" "docstring"

def long_docstring():
    "Loooooooooooooooooooooong" "doooooooooooooooooooocstriiiiiiiiiiiiiiiiiiiiiiiiiiiiiiing" "exceeding the line width" "but it should be concatenated anyways because it is single line"

def docstring_with_leading_whitespace():
    "    This is a " "implicit" "concatenated" "docstring"

def docstring_with_trailing_whitespace():
    "This is a " "implicit" "concatenated" "docstring    "

def docstring_with_leading_empty_parts():
    "       " "   " "" "This is a " "implicit" "concatenated" "docstring"

def docstring_with_trailing_empty_parts():
    "This is a " "implicit" "concatenated" "docstring" ""   "  " "           "

def all_empty():
    "          " "       " " "

def byte_string_in_docstring_position():
    b"  don't trim the" b"bytes literal "

def f_string_in_docstring_position():
    f"  don't trim the" "f-string literal "

def single_quoted():
    ' content\ ' '     '
    return

def implicit_with_comment():
    (
        "a"
        # leading
        "the comment above"
    )

##############################################################################
# Regressions
##############################################################################

LEEEEEEEEEEEEEEEEEEEEEEFT = RRRRRRRRIIIIIIIIIIIIGGGGGHHHT | {
    "entityNameeeeeeeeeeeeeeeeee",  # comment must be long enough to
    "some long implicit concatenated string" "that should join"
}

# Ensure that flipping between Multiline and BestFit layout results in stable formatting
# when using IfBreaksParenthesized layout.
assert False, "Implicit concatenated string" "uses {} layout on {} format".format(
    "Multiline", "first"
)

assert False, await "Implicit concatenated string" "uses {} layout on {} format".format(
    "Multiline", "first"
)

assert False, "Implicit concatenated stringuses {} layout on {} format"[
    aaaaaaaaa, bbbbbb
]

assert False, +"Implicit concatenated string" "uses {} layout on {} format".format(
    "Multiline", "first"
)


# Regression tests for https://github.com/astral-sh/ruff/issues/13935

"a" f'{1=: "abcd \'\'}'
f'{1=: "abcd \'\'}' "a"
f'{1=: "abcd \'\'}' f"{1=: 'abcd \"\"}"

# These strings contains escaped newline characters and should be joined, they are
# not multiline strings.
f"aaaaaaaaaaaaaaaa \
        bbbbbbbbbbb" "cccccccccccccc \
               ddddddddddddddddddd"
b"aaaaaaaaaaaaaaaa \
        bbbbbbbbbbb" b"cccccccccccccc \
               ddddddddddddddddddd"
f"aaaaaaaaaaaaaaaa \
        bbbbbbbbbbb" "cccccccccccccc \
               ddddddddddddddddddd"  # comment 1
(f"aaaaaaaaaaaaaaaa \
        bbbbbbbbbbb" "cccccccccccccc \
               ddddddddddddddddddd")  # comment 2
(
    f"aaaaaaaaaaaaaaaa \
            bbbbbbbbbbb" # comment 3
    "cccccccccccccc \
            ddddddddddddddddddd"  # comment 4
)
