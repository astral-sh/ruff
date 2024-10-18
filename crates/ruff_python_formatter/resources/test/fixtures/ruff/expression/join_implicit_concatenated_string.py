"aaaaaaaaa" "bbbbbbbbbbbbbbbbbbbb" # Join

(
    "aaaaaaaaaaa" "bbbbbbbbbbbbbbbb"
) # join


(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
) # too long to join

"diffent '" 'quote "are fine"' # join


b"aaaaaaaaa" b"bbbbbbbbbbbbbbbbbbbb" # Join

(
    b"aaaaaaaaaaa" b"bbbbbbbbbbbbbbbb"
) # join


(
    b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" b"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
) # too long to join

##############################################################################
# F-strings
##############################################################################

# Escape `{` and `}` when marging an f-string with a string
"a {not_a_variable}" f"b {10}" "c"

f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa{
    expression
}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" f"cccccccccccccccccccc {20999}" "more"

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
