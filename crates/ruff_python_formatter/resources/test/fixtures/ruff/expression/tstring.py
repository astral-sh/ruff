(
    t'{one}'
    t'{two}'
)


rt"Not-so-tricky \"quote"

# Regression test for tstrings dropping comments
result_f = (
    t'Traceback (most recent call last):\n'
    t'  File "{__file__}", line {lineno_f+5}, in _check_recursive_traceback_display\n'
    t'    f()\n'
    t'  File "{__file__}", line {lineno_f+1}, in f\n'
    t'    f()\n'
    t'  File "{__file__}", line {lineno_f+1}, in f\n'
    t'    f()\n'
    t'  File "{__file__}", line {lineno_f+1}, in f\n'
    t'    f()\n'
    # XXX: The following line changes depending on whether the tests
    # are run through the interactive interpreter or with -m
    # It also varies depending on the platform (stack size)
    # Fortunately, we don't care about exactness here, so we use regex
    rt'  \[Previous line repeated (\d+) more times\]' t'\n'
    t'RecursionError: maximum recursion depth exceeded\n'
)


# Regression for tstring dropping comments that were accidentally attached to
# an expression inside a formatted value
(
    t'{1}'
    # comment 1
    t''
)

(
    t'{1}'  # comment 2
    t'{2}'
)

(
    t'{1}'
    t'{2}'  # comment 3
)

(
    1, (  # comment 4
        t'{2}'
    )
)

(
    (
        t'{1}'
        # comment 5
    ),
    2
)

# https://github.com/astral-sh/ruff/issues/6841
x = t'''a{""}b'''
y = t'''c{1}d"""e'''
z = t'''a{""}b''' t'''c{1}d"""e'''

# T-String formatting test cases (Preview)

# Simple expression with a mix of debug expression and comments.
x = t"{a}"
x = t"{
    a = }"
x = t"{ # comment 6
    a }"
x = t"{   # comment 7
    a = }"

# Remove the parentheses as adding them doesn't make then fit within the line length limit.
# This is similar to how we format it before t-string formatting.
aaaaaaaaaaa = (
    t"asaaaaaaaaaaaaaaaa { aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc + dddddddd } cccccccccc"
)
# Here, we would use the best fit layout to put the t-string indented on the next line
# similar to the next example.
aaaaaaaaaaa = t"asaaaaaaaaaaaaaaaa { aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc } cccccccccc"
aaaaaaaaaaa = (
    t"asaaaaaaaaaaaaaaaa { aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc } cccccccccc"
)

# This should never add the optional parentheses because even after adding them, the
# t-string exceeds the line length limit.
x = t"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" } ccccccccccccccc"
x = t"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" = } ccccccccccccccc"
x = t"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment 8
                                             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" } ccccccccccccccc"
x = t"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment 9
                                             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" = } ccccccccccccccc"
x = t"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment 9
                                             'bbbbbbbbbbbbbbbbbbbbbbbbbbbbb' = } ccccccccccccccc"

# Multiple larger expressions which exceeds the line length limit. Here, we need to decide
# whether to split at the first or second expression. This should work similarly to the
# assignment statement formatting where we split from right to left in preview mode.
x = t"aaaaaaaaaaaa { bbbbbbbbbbbbbb } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"

# The above example won't split but when we start introducing line breaks:
x = t"aaaaaaaaaaaa {
        bbbbbbbbbbbbbb } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = t"aaaaaaaaaaaa { bbbbbbbbbbbbbb
                    } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = t"aaaaaaaaaaaa { bbbbbbbbbbbbbb } cccccccccccccccccccc {
        ddddddddddddddd } eeeeeeeeeeeeee"
x = t"aaaaaaaaaaaa { bbbbbbbbbbbbbb } cccccccccccccccccccc { ddddddddddddddd
                                                            } eeeeeeeeeeeeee"

# But, in case comments are present, we would split at the expression containing the
# comments:
x = t"aaaaaaaaaaaa { bbbbbbbbbbbbbb # comment 10
                    } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = t"aaaaaaaaaaaa { bbbbbbbbbbbbbb
                    } cccccccccccccccccccc { # comment 11
                                            ddddddddddddddd } eeeeeeeeeeeeee"

# Here, the expression part itself starts with a curly brace so we need to add an extra
# space between the opening curly brace and the expression.
x = t"{ {'x': 1, 'y': 2} }"
# Although the extra space isn't required before the ending curly brace, we add it for
# consistency.
x = t"{ {'x': 1, 'y': 2}}"
x = t"{ {'x': 1, 'y': 2} = }"
x = t"{  # comment 12
    {'x': 1, 'y': 2} }"
x = t"{    # comment 13
    {'x': 1, 'y': 2} = }"
# But, if there's a format specifier or a conversion flag then we don't need to add
# any whitespace at the end
x = t"aaaaa { {'aaaaaa', 'bbbbbbb', 'ccccccccc'}!s} bbbbbb"
x = t"aaaaa { {'aaaaaa', 'bbbbbbb', 'ccccccccc'}:.3f} bbbbbb"

# But, in this case, we would split the expression itself because it exceeds the line
# length limit so we need not add the extra space.
xxxxxxx = t"{
    {'aaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbb', 'ccccccccccccccccccccc'}
}"
# And, split the expression itself because it exceeds the line length.
xxxxxxx = t"{
    {'aaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccc'}
}"

#############################################################################################
# Quotes
#############################################################################################
t"foo 'bar' {x}"
t"foo \"bar\" {x}"
t'foo "bar" {x}'
t'foo \'bar\' {x}'
t"foo {"bar"}"

t"single quoted '{x}' double quoted \"{x}\"" # Same number of quotes => use preferred quote style
t"single quote ' {x} double quoted \"{x}\""  # More double quotes => use single quotes
t"single quoted '{x}' double quote \" {x}"  # More single quotes => use double quotes

fr"single quotes ' {x}"  # Keep double because `'` can't be escaped
fr'double quotes " {x}'  # Keep single because `"` can't be escaped
fr'flip quotes {x}'  # Use preferred quotes, because raw string contains now quotes.

# Here, the formatter will remove the escapes
t"foo {'\'bar\''}"
t"foo {'\"bar\"'}"

# Quotes inside the expressions have no impact on the quote selection of the outer string.
# Required so that the following two examples result in the same formatting.
t'foo {10 + len("bar")}'
t"foo {10 + len('bar')}"

# Pre 312, preserve the outer quotes if the t-string contains quotes in the debug expression
t'foo {10 + len("bar")=}'
t'''foo {10 + len('''bar''')=}'''
t'''foo {10 + len('bar')=}'''  # Fine to change the quotes because it uses triple quotes

# Triple-quoted strings
# It's ok to use the same quote char for the inner string if it's single-quoted.
t"""test {'inner'}"""
t"""test {"inner"}"""
# But if the inner string is also triple-quoted then we should preserve the existing quotes.
t"""test {'''inner'''}"""

# It's not okay to change the quote style if the inner string is triple quoted and contains a quote.
t'{"""other " """}'
t'{"""other " """ + "more"}'
t'{b"""other " """}'
t'{t"""other " """}'

t"""test {t'inner {'''inner inner'''}'}"""
t"""test {t'''inner {"""inner inner"""}'''}"""

# Magic trailing comma
#
# The expression formatting will result in breaking it across multiple lines with a
# trailing comma but as the expression isn't already broken, we will remove all the line
# breaks which results in the trailing comma being present. This test case makes sure
# that the trailing comma is removed as well.
t"aaaaaaa {['aaaaaaaaaaaaaaa', 'bbbbbbbbbbbbb', 'ccccccccccccccccc', 'ddddddddddddddd', 'eeeeeeeeeeeeee']} aaaaaaa"

# And, if the trailing comma is already present, we still need to remove it.
t"aaaaaaa {['aaaaaaaaaaaaaaa', 'bbbbbbbbbbbbb', 'ccccccccccccccccc', 'ddddddddddddddd', 'eeeeeeeeeeeeee',]} aaaaaaa"

# Keep this Multiline by breaking it at the square brackets.
t"""aaaaaa {[
    xxxxxxxx,
    yyyyyyyy,
]} ccc"""

# Add the magic trailing comma because the elements don't fit within the line length limit
# when collapsed.
t"aaaaaa {[
    xxxxxxxxxxxx,
    xxxxxxxxxxxx,
    xxxxxxxxxxxx,
    xxxxxxxxxxxx,
    xxxxxxxxxxxx,
    xxxxxxxxxxxx,
    yyyyyyyyyyyy
]} ccccccc"

# Remove the parentheses because they aren't required
xxxxxxxxxxxxxxx = (
    t"aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbb {
        xxxxxxxxxxx  # comment 14
        + yyyyyyyyyy
    } dddddddddd"
)

# Comments

# No comments should be dropped!
t"{ # comment 15
    # comment 16
    foo # comment 17
    # comment 18
}"  # comment 19
# comment 20

# The specifier of a t-string must hug the closing `}` because a multiline format specifier is invalid syntax in a single
# quoted f-string.
t"aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {
    variable
    :.3f} ddddddddddddddd eeeeeeee"

# The same applies for triple quoted t-strings, except that we need to preserve the newline before the closing `}`.
# or we risk altering the meaning of the f-string.
t"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {variable
    :.3f} ddddddddddddddd eeeeeeee"""
t"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {variable
    :.3f
} ddddddddddddddd eeeeeeee"""


# Throw in a random comment in it but surprise, this is not a comment but just a text
# which is part of the format specifier
aaaaaaaaaaa = t"""asaaaaaaaaaaaaaaaa {
        aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc + dddddddd:.3f
     # comment
} cccccccccc"""
aaaaaaaaaaa = t"""asaaaaaaaaaaaaaaaa {
        aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc + dddddddd:.3f
     # comment} cccccccccc"""

# Conversion flags
#

# Even in the case of debug expressions, we only need to preserve the whitespace within
# the expression part of the replacement field.
x = t"aaaaaaaaa { x   = !r  }"

# Combine conversion flags with format specifiers
x = t"{x   =   !s
         :>0}"

x = f"{
    x!s:>{
        0
        # comment 21-2
    }}"

f"{1
    # comment 21-3
:}"

f"{1 # comment 21-4
:} a"

x = t"""
{              # comment 22
 x =   :.0{y # comment 23
           }f}"""

# Here, the debug expression is in a nested t-string so we should start preserving
# whitespaces from that point onwards. This means we should format the outer t-string.
x = t"""{"foo " +    # comment 24
    t"{   x =

       }"    # comment 25
 }
        """

# Mix of various features.
t"""{  # comment 26
    foo # after foo
   :>{
          x # after x
          }
    # comment 27
    # comment 28
} woah {x}"""

# Assignment statement

# Even though this t-string has multiline expression, thus allowing us to break it at the
# curly braces, the t-string fits on a single line if it's moved inside the parentheses.
# We should prefer doing that instead.
aaaaaaaaaaaaaaaaaa = t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
    expression}moreeeeeeeeeeeeeeeee"

# Same as above
xxxxxxx = t"{
    {'aaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccc'}
}"

# Similar to the previous example, but the t-string will exceed the line length limit,
# we shouldn't add any parentheses here.
xxxxxxx = t"{
    {'aaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccc'}
}"

# Same as above but with an inline comment. The t-string should be formatted inside the
# parentheses and the comment should be part of the line inside the parentheses.
aaaaaaaaaaaaaaaaaa = t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
    expression}moreeeeeeeeeeeeeeeee"  # comment

# Similar to the previous example but this time parenthesizing doesn't work because it
# exceeds the line length. So, avoid parenthesizing this t-string.
aaaaaaaaaaaaaaaaaa = t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
    expression}moreeeeeeeeeeeeeeeee"  # comment loooooooong

# Similar to the previous example but we start with the parenthesized layout. This should
# remove the parentheses and format the t-string on a single line. This shows that the
# final layout for the formatter is same for this and the previous case. The only
# difference is that in the previous case the expression is already mulitline which means
# the formatter can break it further at the curly braces.
aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeee"  # comment loooooooong
)

# The following t-strings are going to break because of the trailing comma so we should
# avoid using the best fit layout and instead use the default layout.
# left-to-right
aaaa = t"aaaa {[
    1, 2,
]} bbbb"
# right-to-left
aaaa, bbbb = t"aaaa {[
    1, 2,
]} bbbb"

# Using the right-to-left assignment statement variant.
aaaaaaaaaaaaaaaaaa, bbbbbbbbbbb = t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
    expression}moreeeeeeeeeeeeeeeee"  # comment

# Here, the t-string layout is flat but it exceeds the line length limit. This shouldn't
# try the custom best fit layout because the t-string doesn't have any split points.
aaaaaaaaaaaa["bbbbbbbbbbbbbbbb"] = (
    t"aaaaaaaaaaaaaaaaaaa {aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc} ddddddddddddddddddd"
)
# Same as above but without the parentheses to test that it gets formatted to the same
# layout as the previous example.
aaaaaaaaaaaa["bbbbbbbbbbbbbbbb"] = t"aaaaaaaaaaaaaaaaaaa {aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc} ddddddddddddddddddd"

# But, the following t-string does have a split point because of the multiline expression.
aaaaaaaaaaaa["bbbbbbbbbbbbbbbb"] = (
    t"aaaaaaaaaaaaaaaaaaa {
        aaaaaaaaa + bbbbbbbbbbb + cccccccccccccc} ddddddddddddddddddd"
)
aaaaaaaaaaaa["bbbbbbbbbbbbbbbb"] = (
    t"aaaaaaaaaaaaaaaaaaa {
        aaaaaaaaaaaaaaaaaaaa + bbbbbbbbbbbbbbbbbbbbb + cccccccccccccccccccccc + dddddddddddddddddddddddddddd} ddddddddddddddddddd"
)

# This is an implicitly concatenated t-string but it cannot be joined because otherwise
# it'll exceed the line length limit. So, the two t-strings will be inside parentheses
# instead and the inline comment should be outside the parentheses.
a = t"test{
    expression
}flat" t"can be {
    joined
} togethereeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee" # inline

# Similar to the above example but this fits within the line length limit.
a = t"test{
    expression
}flat" t"can be {
    joined
} togethereeeeeeeeeeeeeeeeeeeeeeeeeee" # inline

# The following test cases are adopted from implicit string concatenation but for a
# single t-string instead.

# Don't inline t-strings that contain expressions that are guaranteed to split, e.g. because of a magic trailing comma
aaaaaaaaaaaaaaaaaa = t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}moreeeeeeeeeeeeeeeeeeee" # comment

aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}moreeeeeeeeeeeeeeeeeeee" # comment
)

aaaaa[aaaaaaaaaaa] = t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}moreeeeeeeeeeeeeeeeeeee" # comment

aaaaa[aaaaaaaaaaa] = (t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}moreeeeeeeeeeeeeeeeeeee" # comment
)

# Don't inline t-strings that contain commented expressions
aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{[
        a  # comment
    ]}moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaa[aaaaaaaaaaa] = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{[
        a  # comment
    ]}moreeeeeeeeeeeeeeeeeetest"  # comment
)

# Don't inline t-strings with multiline debug expressions or format specifiers
aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{
    a=}moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{a +
    b=}moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{a
    =}moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaa[aaaaaaaaaaa] = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{
    a=}moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaa[aaaaaaaaaaa] = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{a
    =}moreeeeeeeeeeeeeeeeeetest"  # comment
)

# This is not a multiline t-string even though it has a newline after the format specifier.
aaaaaaaaaaaaaaaaaa = t"testeeeeeeeeeeeeeeeeeeeeeeeee{
    a:.3f}moreeeeeeeeeeeeeeeeeetest"  # comment

aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeee{
    a:.3f}moreeeeeeeeeeeeeeeeeetest"  # comment
)

# The newline is only considered when it's a tripled-quoted t-string.
aaaaaaaaaaaaaaaaaa = t"""testeeeeeeeeeeeeeeeeeeeeeeeee{
    a:.3f
    }moreeeeeeeeeeeeeeeeeetest"""  # comment

aaaaaaaaaaaaaaaaaa = (
    t"""testeeeeeeeeeeeeeeeeeeeeeeeee{
    a:.3f
    }moreeeeeeeeeeeeeeeeeetest"""  # comment
)

# Remove the parentheses here
aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{[a, b,
    # comment
    ]}moee" # comment
)
# ... but not here because of the ownline comment
aaaaaaaaaaaaaaaaaa = (
    t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{[a, b,
    ]}moee"
    # comment
)

# t-strings in other positions

if t"aaaaaaaaaaa {ttttteeeeeeeeest} more {
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
}": pass

if (
    t"aaaaaaaaaaa {ttttteeeeeeeeest} more {
        aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    }"
): pass

if t"aaaaaaaaaaa {ttttteeeeeeeeest} more {
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
}": pass

if t"aaaaaaaaaaa {ttttteeeeeeeeest} more {  # comment
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
}": pass

if t"aaaaaaaaaaa {[ttttteeeeeeeeest,]} more {
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
}":
    pass

if (
    t"aaaaaaaaaaa {[ttttteeeeeeeeest,]} more {
        aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    }"
):
    pass

if t"aaaaaaaaaaa {[ttttteeeeeeeeest,]} more {
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
}":
    pass

# For loops
for a in t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
        expression}moreeeeeeeeeeeeeeeeeeee":
    pass

for a in t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeeeeeeeeeeeeeeee":
    pass

for a in t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
        expression}moreeeeeeeeeeeeeeeeeeeeeeeeeeeeee":
    pass

for a in (
    t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeeeeeeeeeeeeeee"
):
    pass

# With statements
with t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
        expression}moreeeeeeeeeeeeeeeeeeee":
    pass

with t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeeeeeeeeeeeeeeee":
    pass

with t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
        expression}moreeeeeeeeeeeeeeeeeeeeeeeeeeeeee":
    pass

with (
    t"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{expression}moreeeeeeeeeeeeeeeeeeeeeeeeeeeee"
):
    pass

# Assert statements
assert t"aaaaaaaaa{
        expression}bbbbbbbbbbbb", t"cccccccccc{
                expression}dddddddddd"

assert t"aaaaaaaaa{expression}bbbbbbbbbbbb", t"cccccccccccccccc{
        expression}dddddddddddddddd"

assert t"aaaaaaaaa{expression}bbbbbbbbbbbb", t"cccccccccccccccc{expression}dddddddddddddddd"

assert t"aaaaaaaaaaaaaaaaaaaaaaaaaaa{
        expression}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", t"ccccccc{expression}dddddddddd"

assert t"aaaaaaaaaaaaaaaaaaaaaaaaaaa{expression}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", t"ccccccc{expression}dddddddddd"

assert t"aaaaaaaaaaaaaaaaaaaaaaaaaaa{
        expression}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", t"ccccccccccccccccccccc {
                expression} dddddddddddddddddddddddddd"

assert t"aaaaaaaaaaaaaaaaaaaaaaaaaaa{expression}bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", t"cccccccccccccccccccccccccccccccc {expression} ddddddddddddddddddddddddddddddddddddd"

# t-strings as a single argument to a call expression to test whether it's huggable or not.
call(t"{
    testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
}")

call(t"{
    testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
}")

call(t"{  # comment
    testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
}")

call(t"""aaaaaaaaaaaaaaaa bbbbbbbbbb {testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee}""")

call(t"""aaaaaaaaaaaaaaaa bbbbbbbbbb {testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
                }""")

call(t"""aaaaaaaaaaaaaaaa
     bbbbbbbbbb {testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
                }""")

call(t"""aaaaaaaaaaaaaaaa
     bbbbbbbbbb {testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  # comment
                }""")

call(
    t"""aaaaaaaaaaaaaaaa
     bbbbbbbbbb {testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee  # comment
                }"""
)

call(t"{
    aaaaaa
    + '''test
    more'''
}")

# Indentation

# What should be the indentation?
# https://github.com/astral-sh/ruff/discussions/9785#discussioncomment-8470590
if indent0:
    if indent1:
        if indent2:
            foo = t"""hello world
hello {
          t"aaaaaaa {
              [
                  'aaaaaaaaaaaaaaaaaaaaa',
                  'bbbbbbbbbbbbbbbbbbbbb',
                  'ccccccccccccccccccccc',
                  'ddddddddddddddddddddd'
              ]
          } bbbbbbbb" +
          [
              'aaaaaaaaaaaaaaaaaaaaa',
              'bbbbbbbbbbbbbbbbbbbbb',
              'ccccccccccccccccccccc',
              'ddddddddddddddddddddd'
          ]
      } --------
"""


# Implicit concatenated t-string containing quotes
_ = (
    t'This string should change its quotes to double quotes'
    t'This string uses double quotes in an expression {"it's a quote"}'
    t'This t-string does not use any quotes.'
)

# Regression test for https://github.com/astral-sh/ruff/issues/14487
t"aaaaaaaaaaaaaaaaaaaaaaaaaa {10**27} bbbbbbbbbbbbbbbbbbbbbbbbbb ccccccccccccccccccccccccc"

# Regression test for https://github.com/astral-sh/ruff/issues/14778
t"{'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' 'a' if True else ""}"
t"{'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' 'a' if True else ""}"

# Quotes reuse
t"{'a'}"

# 312+, it's okay to change the outer quotes even when there's a debug expression using the same quotes
t'foo {10 + len("bar")=}'
t'''foo {10 + len("""bar""")=}'''

# 312+, it's okay to change the quotes here without creating an invalid t-string
t'{"""other " """}'
t'{"""other " """ + "more"}'
t'{b"""other " """}'
t'{t"""other " """}'


# Regression tests for https://github.com/astral-sh/ruff/issues/13935
t'{1: hy "user"}'
t'{1:hy "user"}'
t'{1: abcd "{1}" }'
t'{1: abcd "{'aa'}" }'
t'{1=: "abcd {'aa'}}'
t'{x:a{z:hy "user"}} \'\'\''

# Changing the outer quotes is fine because the format-spec is in a nested expression.
t'{t'{z=:hy "user"}'} \'\'\''


#  We have to be careful about changing the quotes if the t-string has a debug expression because it is inserted verbatim.
t'{1=: "abcd \'\'}'  # Don't change the outer quotes, or it results in a syntax error
t'{1=: abcd \'\'}'  # Changing the quotes here is fine because the inner quotes aren't the opposite quotes
t'{1=: abcd \"\"}'  # Changing the quotes here is fine because the inner quotes are escaped
# Don't change the quotes in the following cases:
t'{x=:hy "user"} \'\'\''
t'{x=:a{y:hy "user"}} \'\'\''
t'{x=:a{y:{z:hy "user"}}} \'\'\''
t'{x:a{y=:{z:hy "user"}}} \'\'\''

# This is fine because the debug expression and format spec are in a nested expression

t"""{1=: "this" is fine}"""
t'''{1=: "this" is fine}'''  # Change quotes to double quotes because they're preferred
t'{1=: {'ab"cd"'}}'  # It's okay if the quotes are in an expression part.


# Regression tests for https://github.com/astral-sh/ruff/issues/15459
print(t"{ {1, 2, 3} - {2} }")
print(t"{ {1: 2}.keys() }")
print(t"{({1, 2, 3}) - ({2})}")
print(t"{1, 2, {3} }")
print(t"{(1, 2, {3})}")


# Regression tests for https://github.com/astral-sh/ruff/issues/15535
print(t"{ {}, }")  # A single item tuple gets parenthesized
print(t"{ {}.values(), }")
print(t"{ {}, 1 }")  # A tuple with multiple elements doesn't get parenthesized
print(t"{  # Tuple with multiple elements that doesn't fit on a single line gets parenthesized
    {}, 1,
}")


# Regression tests for https://github.com/astral-sh/ruff/issues/15536
print(t"{ {}, 1, }")
