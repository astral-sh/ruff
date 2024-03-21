# See https://docs.python.org/3/reference/expressions.html#operator-precedence
# for the official docs on operator precedence.
#
# Most importantly, `and` *always* takes precedence over `or`.
#
# `not` (the third boolean/logical operator) takes precedence over both,
# but the rule there is easier to remember,
# so we don't emit a diagnostic if a `not` expression is unparenthesized
# as part of a chain.

a, b, c = 1, 0, 2
x = a or b and c  # RUF021: => `a or (b and c)`
x = a or b and c  # looooooooooooooooooooooooooooooong comment but it won't prevent an autofix

a, b, c = 0, 1, 2
y = a and b or c  # RUF021: => `(a and b) or c`

a, b, c, d = 1, 2, 0, 3
if a or b or c and d:  # RUF021: => `a or b or (c and d)`
    pass

a, b, c, d = 0, 0, 2, 3

if bool():
    pass
elif a or b and c or d:  # RUF021: => `a or (b and c) or d`
    pass

a, b, c, d = 0, 1, 0, 2
while a and b or c and d:  # RUF021: => `(and b) or (c and d)`
    pass

b, c, d, e = 2, 3, 0, 4
# RUF021: => `a or b or c or (d and e)`:
z = [a for a in range(5) if a or b or c or d and e]

a, b, c, d = 0, 1, 3, 0
assert not a and b or c or d  # RUF021: => `(not a and b) or c or d`

if (not a) and b or c or d:  # RUF021: => `((not a) and b) or c or d`
    if (not a and b) or c or d:  # OK
        pass

if (
    some_reasonably_long_condition
    or some_other_reasonably_long_condition
    and some_third_reasonably_long_condition
    or some_fourth_reasonably_long_condition
    and some_fifth_reasonably_long_condition
    # a comment
    and some_sixth_reasonably_long_condition
    and some_seventh_reasonably_long_condition
    # another comment
    or some_eighth_reasonably_long_condition
):
    pass

#############################################
# If they're all the same operator, it's fine
#############################################

x = not a and c  # OK

if a or b or c:  # OK
    pass

while a and b and c:  # OK
    pass

###########################################################
# We don't consider `not` as part of a chain as problematic
###########################################################

x = not a or not b or not c  # OK

#####################################
# If they're parenthesized, it's fine
#####################################

a, b, c = 1, 0, 2
x = a or (b and c)  # OK
x2 = (a or b) and c  # OK
x3 = (a or b) or c  # OK
x4 = (a and b) and c  # OK

a, b, c = 0, 1, 2
y = (a and b) or c  # OK
yy = a and (b or c)  # OK

a, b, c, d = 1, 2, 0, 3
if a or b or (c and d):  # OK
    pass

a, b, c, d = 0, 0, 2, 3

if bool():
    pass
elif a or (b and c) or d:  # OK
    pass

a, b, c, d = 0, 1, 0, 2
while (a and b) or (c and d):  # OK
    pass

b, c, d, e = 2, 3, 0, 4
z = [a for a in range(5) if a or b or c or (d and e)]  # OK

a, b = 1, 2
if (not a) or b:  # OK
    if (not a) and b:  # OK
        pass

a, b, c, d = 0, 1, 3, 0
assert ((not a) and b) or c or d  # OK
