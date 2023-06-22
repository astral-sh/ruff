if x == y: # trailing if condition
    pass # trailing `pass` comment
    # Root `if` trailing comment

# Leading elif comment
elif x < y: # trailing elif condition
    pass
    # `elif` trailing comment

# Leading else comment
else: # trailing else condition
    pass
    # `else` trailing comment


if x == y:
    if y == z:
        ...

    if a == b:
        ...
    else: # trailing comment
        ...

    # trailing else comment

# leading else if comment
elif aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + [
    11111111111111111111111111,
    2222222222222222222222,
    3333333333
    ]:
    ...


else:
    ...

# Regression test: Don't drop the trailing comment by associating it with the elif
# instead of the else.
# Originally found in https://github.com/python/cpython/blob/ab3823a97bdeefb0266b3c8d493f7f6223ce3686/Lib/dataclasses.py#L539

if "if 1":
    pass
elif "elif 1":
    pass
# Don't drop this comment 1
x = 1

if "if 2":
    pass
elif "elif 2":
    pass
else:
    pass
# Don't drop this comment 2
x = 2

if "if 3":
    pass
else:
    pass
# Don't drop this comment 3
x = 3

# Regression test for a following if that could get confused for an elif
# Originally found in https://github.com/gradio-app/gradio/blob/1570b94a02d23d051ae137e0063974fd8a48b34e/gradio/external.py#L478
if True:
    pass
else:  # Comment
    if False:
        pass
    pass
