(
    f'{one}'
    f'{two}'
)


rf"Not-so-tricky \"quote"

# Regression test for fstrings dropping comments
result_f = (
    'Traceback (most recent call last):\n'
    f'  File "{__file__}", line {lineno_f+5}, in _check_recursive_traceback_display\n'
    '    f()\n'
    f'  File "{__file__}", line {lineno_f+1}, in f\n'
    '    f()\n'
    f'  File "{__file__}", line {lineno_f+1}, in f\n'
    '    f()\n'
    f'  File "{__file__}", line {lineno_f+1}, in f\n'
    '    f()\n'
    # XXX: The following line changes depending on whether the tests
    # are run through the interactive interpreter or with -m
    # It also varies depending on the platform (stack size)
    # Fortunately, we don't care about exactness here, so we use regex
    r'  \[Previous line repeated (\d+) more times\]' '\n'
    'RecursionError: maximum recursion depth exceeded\n'
)


# Regression for fstring dropping comments that were accidentally attached to
# an expression inside a formatted value
(
    f'{1}'
    # comment 1
    ''
)

(
    f'{1}'  # comment 2
    f'{2}'
)

(
    f'{1}'
    f'{2}'  # comment 3
)

(
    1, (  # comment 4
        f'{2}'
    )
)

(
    (
        f'{1}'
        # comment 5
    ),
    2
)

# https://github.com/astral-sh/ruff/issues/6841
x = f'''a{""}b'''
y = f'''c{1}d"""e'''
z = f'''a{""}b''' f'''c{1}d"""e'''

# F-String formatting test cases (Preview)

# Simple expression with a mix of debug expression and comments.
x = f"{a}"
x = f"{
    a = }"
x = f"{ # comment 6
    a }"
x = f"{   # comment 7
    a = }"

# Remove the parentheses as adding them doesn't make then fit within the line length limit.
# This is similar to how we format it before f-string formatting.
aaaaaaaaaaa = (
    f"asaaaaaaaaaaaaaaaa { aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc + dddddddd } cccccccccc"
)
# Here, we would use the best fit layout to put the f-string indented on the next line
# similar to the next example.
aaaaaaaaaaa = f"asaaaaaaaaaaaaaaaa { aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc } cccccccccc"
aaaaaaaaaaa = (
    f"asaaaaaaaaaaaaaaaa { aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc } cccccccccc"
)

# This should never add the optional parentheses because even after adding them, the
# f-string exceeds the line length limit.
x = f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" } ccccccccccccccc"
x = f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" = } ccccccccccccccc"
x = f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment 8
                                             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" } ccccccccccccccc"
x = f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment 9
                                             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" = } ccccccccccccccc"

# Multiple larger expressions which exceeds the line length limit. Here, we need to decide
# whether to split at the first or second expression. This should work similarly to the
# assignment statement formatting where we split from right to left in preview mode.
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"

# The above example won't split but when we start introducing line breaks:
x = f"aaaaaaaaaaaa {
        bbbbbbbbbbbbbb } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb
                    } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb } cccccccccccccccccccc {
        ddddddddddddddd } eeeeeeeeeeeeee"
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb } cccccccccccccccccccc { ddddddddddddddd
                                                            } eeeeeeeeeeeeee"

# But, in case comments are present, we would split at the expression containing the
# comments:
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb # comment 10
                    } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb
                    } cccccccccccccccccccc { # comment 11
                                            ddddddddddddddd } eeeeeeeeeeeeee"

# Here, the expression part itself starts with a curly brace so we need to add an extra
# space between the opening curly brace and the expression.
x = f"{ {'x': 1, 'y': 2} }"
# Although the extra space isn't required before the ending curly brace, we add it for
# consistency.
x = f"{ {'x': 1, 'y': 2}}"
x = f"{ {'x': 1, 'y': 2} = }"
x = f"{  # comment 12
    {'x': 1, 'y': 2} }"
x = f"{    # comment 13
    {'x': 1, 'y': 2} = }"
# But, if there's a format specifier or a conversion flag then we don't need to add
# any whitespace at the end
x = f"aaaaa { {'aaaaaa', 'bbbbbbb', 'ccccccccc'}!s} bbbbbb"
x = f"aaaaa { {'aaaaaa', 'bbbbbbb', 'ccccccccc'}:.3f} bbbbbb"

# But, in this case, we would split the expression itself because it exceeds the line
# length limit so we need not add the extra space.
xxxxxxx = f"{
    {'aaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbb', 'ccccccccccccccccccccc'}
}"
# And, split the expression itself because it exceeds the line length.
xxxxxxx = f"{
    {'aaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccc'}
}"

# Quotes
f"foo 'bar' {x}"
f"foo \"bar\" {x}"
f'foo "bar" {x}'
f'foo \'bar\' {x}'
f"foo {"bar"}"
f"foo {'\'bar\''}"

# Here, the formatter will remove the escapes which is correct because they aren't allowed
# pre 3.12. This means we can assume that the f-string is used in the context of 3.12.
f"foo {'\"bar\"'}"


# Triple-quoted strings
# It's ok to use the same quote char for the inner string if it's single-quoted.
f"""test {'inner'}"""
f"""test {"inner"}"""
# But if the inner string is also triple-quoted then we should preserve the existing quotes.
f"""test {'''inner'''}"""

# Magic trailing comma
#
# The expression formatting will result in breaking it across multiple lines with a
# trailing comma but as the expression isn't already broken, we will remove all the line
# breaks which results in the trailing comma being present. This test case makes sure
# that the trailing comma is removed as well.
f"aaaaaaa {['aaaaaaaaaaaaaaa', 'bbbbbbbbbbbbb', 'ccccccccccccccccc', 'ddddddddddddddd', 'eeeeeeeeeeeeee']} aaaaaaa"

# And, if the trailing comma is already present, we still need to remove it.
f"aaaaaaa {['aaaaaaaaaaaaaaa', 'bbbbbbbbbbbbb', 'ccccccccccccccccc', 'ddddddddddddddd', 'eeeeeeeeeeeeee',]} aaaaaaa"

# Keep this Multiline by breaking it at the square brackets.
f"""aaaaaa {[
    xxxxxxxx,
    yyyyyyyy,
]} ccc"""

# Add the magic trailing comma because the elements don't fit within the line length limit
# when collapsed.
f"aaaaaa {[
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
    f"aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbb {
        xxxxxxxxxxx  # comment 14
        + yyyyyyyyyy
    } dddddddddd"
)

# Comments

# No comments should be dropped!
f"{ # comment 15
    # comment 16
    foo # comment 17
    # comment 18
}"  # comment 19
# comment 20

# Single-quoted f-strings with a format specificer can be multiline
f"aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {
    variable:.3f} ddddddddddddddd eeeeeeee"

# But, if it's triple-quoted then we can't other the format specificer will have a
# trailing newline
f"""aaaaaaaaaaaaaaaa bbbbbbbbbbbbbbbbbb ccccccccccc {
    variable:.3f} ddddddddddddddd eeeeeeee"""

# But, we can break the ones which don't have a format specifier
f"""fooooooooooooooooooo barrrrrrrrrrrrrrrrrrr {
        xxxxxxxxxxxxxxx:.3f} aaaaaaaaaaaaaaaaa { xxxxxxxxxxxxxxxxxxxx } bbbbbbbbbbbb"""

# Throw in a random comment in it but surpise, this is not a comment but just a text
# which is part of the format specifier
aaaaaaaaaaa = f"""asaaaaaaaaaaaaaaaa {
        aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc + dddddddd:.3f
     # comment
} cccccccccc"""
aaaaaaaaaaa = f"""asaaaaaaaaaaaaaaaa {
        aaaaaaaaaaaa + bbbbbbbbbbbb + ccccccccccccccc + dddddddd:.3f
     # comment} cccccccccc"""

# Conversion flags
#
# This is not a valid Python code because of the additional whitespace between the `!`
# and conversion type. But, our parser isn't strict about this. This should probably be
# removed once we have a strict parser.
x = f"aaaaaaaaa { x !  r }"

# Even in the case of debug expressions, we only need to preserve the whitespace within
# the expression part of the replacement field.
x = f"aaaaaaaaa { x   = !  r  }"

# Combine conversion flags with format specifiers
x = f"{x   =   !  s
         :>0

         }"
# This is interesting. There can be a comment after the format specifier but only if it's
# on it's own line. Refer to https://github.com/astral-sh/ruff/pull/7787 for more details.
# We'll format is as trailing comments.
x = f"{x  !s
         :>0
         # comment 21
         }"

x = f"""
{              # comment 22
 x =   :.0{y # comment 23
           }f}"""

# Here, the debug expression is in a nested f-string so we should start preserving
# whitespaces from that point onwards. This means we should format the outer f-string.
x = f"""{"foo " +    # comment 24
    f"{   x =

       }"    # comment 25
 }
        """

# Mix of various features.
f"{  # comment 26
    foo # after foo
   :>{
          x # after x
          }
    # comment 27
    # comment 28
} woah {x}"

# Indentation

# What should be the indentation?
# https://github.com/astral-sh/ruff/discussions/9785#discussioncomment-8470590
if indent0:
    if indent1:
        if indent2:
            foo = f"""hello world
hello {
          f"aaaaaaa {
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
