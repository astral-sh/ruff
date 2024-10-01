if x is "abc":
    pass

if 123 is not y:
    pass

if 123 is \
        not y:
    pass

if "123" is x < 3:
    pass

if "123" != x is 3:
    pass

if ("123" != x) is 3:
    pass

if "123" != (x is 3):
    pass

{2 is
not ''}

{2 is
 not ''}

# Regression test for
if values[1is not None ] is not '-':
    pass

# Regression test for https://github.com/astral-sh/ruff/issues/11736
variable: "123 is not y"
