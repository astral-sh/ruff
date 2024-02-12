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
    # comment
    ''
)

(
    f'{1}'  # comment
    f'{2}'
)

(
    f'{1}'
    f'{2}'  # comment
)

(
    1, (  # comment
        f'{2}'
    )
)

(
    (
        f'{1}'
        # comment
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
x = f"{ # comment
    a }"
x = f"{   # comment
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
x = f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment
                                             "bbbbbbbbbbbbbbbbbbbbbbbbbbbbb" } ccccccccccccccc"
x = f"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa { # comment
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
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb # comment
                    } cccccccccccccccccccc { ddddddddddddddd } eeeeeeeeeeeeee"
x = f"aaaaaaaaaaaa { bbbbbbbbbbbbbb
                    } cccccccccccccccccccc { # comment
                                            ddddddddddddddd } eeeeeeeeeeeeee"

# Here, the expression part itself starts with a curly brace so we need to add an extra
# space between the opening curly brace and the expression.
x = f"{ {'x': 1, 'y': 2} }"
# Although the extra space isn't required before the ending curly brace, we add it for
# consistency.
x = f"{ {'x': 1, 'y': 2}}"
x = f"{ {'x': 1, 'y': 2} = }"
x = f"{  # comment
    {'x': 1, 'y': 2} }"
x = f"{    # comment
    {'x': 1, 'y': 2} = }"

# But, in this case, we would split the expression itself because it exceeds the line
# length limit so we need not add the extra space.
xxxxxxx = f"{
    {'aaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbb', 'ccccccccccccccccccccc'}
}"
# And, split the expression itself because it exceeds the line length.
xxxxxxx = f"{
    {'aaaaaaaaaaaaaaaaaaaaaaaaa', 'bbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccc'}
}"

# Triple-quoted strings
# It's ok to use the same quote char for the inner string if it's single-quoted.
f"""test {'inner'}"""
f"""test {"inner"}"""
# But if the inner string is also triple-quoted then we should preserve the existing quotes.
f"""test {'''inner'''}"""

# Comments

# No comments should be dropped!
f"{ # comment 1
    # comment 2
    foo # comment 3
    # comment 4
}"  # comment 5
# comment 6

# Conversion flags
#
# This is not a valid Python code because of the additional whitespace between the `!`
# and conversion type. But, our parser isn't strict about this. This should probably be
# removed once we have a strict parser.
x = f"aaaaaaaaa { x !  r }"

# Even in the case of debug expresions, we only need to preserve the whitespace within
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
         # comment
         }"

x = f"""
{              # dangling comment 1
 x =   :.0{y # dangling comment 2
           }f}"""

# Here, the debug expression is in a nested f-string so we should start preserving
# whitespaces from that point onwards. This means we should format the outer f-string.
x = f"""{"foo " +    # comment 1
    f"{   x =

       }"    # comment 2
 }
        """

# Mix of various features.
f"{  # dangling comment 1
    foo # after foo
   :>{
          x # after x
          }
    # dangling comment 2
    # dangling comment 3
} woah {x}"
