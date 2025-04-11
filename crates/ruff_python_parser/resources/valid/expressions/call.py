# This only tests the expression before the opening parenthesis for the call expression
# and not the arguments.

call()
attr.expr()
subscript[1, 2]()
slice[:1]()
[1, 2, 3]()
(1, 2, 3)()
(x for x in iter)()
{1, 2, 3}()
{1: 2, 3: 4}()
(yield x)()

# These are `TypeError`, so make sure it parses correctly.
True()
False()
None()
"string"()
1()
1.0()
