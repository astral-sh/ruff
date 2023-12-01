# 1 leading if comment
if x == y:  # 2 trailing if condition
    # 3 leading pass
    pass  # 4 end-of-line trailing `pass` comment
    # 5 Root `if` trailing comment

# 6 Leading elif comment
elif x < y:  # 7 trailing elif condition
    # 8 leading pass
    pass # 9 end-of-line trailing `pass` comment
    # 10 `elif` trailing comment

# 11 Leading else comment
else:  # 12 trailing else condition
    # 13 leading pass
    pass # 14 end-of-line trailing `pass` comment
    # 15 `else` trailing comment


if x == y:
    if y == z:
        pass

    if a == b:
        pass
    else: # trailing comment
        pass

    # trailing else comment

# leading else if comment
elif aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + [
    11111111111111111111111111,
    2222222222222222222222,
    3333333333
    ]:
    pass


else:
    pass

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


# Regression test for `last_child_in_body` special casing of `StmtIf`
# https://github.com/python/cpython/blob/aecf6aca515a203a823a87c711f15cbb82097c8b/Lib/test/test_pty.py#L260-L275
def f():
    if True:
        pass
    else:
        pass

        # comment

if True:
    def f():
        pass
        # 1
elif True:
    def f():
        pass
        # 2
else:
    def f():
        pass
        # 3

if True: print("a") # 1
elif True: print("b") # 2
else: print("c") # 3

# Regression test for: https://github.com/astral-sh/ruff/issues/7465
if True:
    pass

    # comment
else:
    pass

if True:
    pass

    # comment

else:
    pass

if True:
    pass
    # comment

else:
    pass

if True:
    pass
    # comment
else:
    pass

if True:
    pass  # comment
else:
    pass

if True:
    pass

    # comment

else:
    pass

if True:
    pass

    # comment


else:
    pass

if True:
    pass

    # comment



else:
    pass

if True:
    if True:
        pass
        # comment

    else:
        pass
else:
    pass

if True:
    if True:
        pass
        # comment


    else:
        pass
else:
    pass

if True:
    pass
# comment
else:
    pass

if True:
    pass

# comment
else:
    pass

if True:
    pass


# comment
else:
    pass

if True:
    if True:
        pass
    else:
        pass
        # a

        # b
        # c

else:
    pass

if True:
    if True:
        pass
    else:
        pass

        # b
        # c

else:
    pass


# Regression test for: https://github.com/astral-sh/ruff/issues/7602
if True:
    if True:
        if True:
            pass

        #a
            #b
        #c
else:
    pass

if True:
    if True:
        if True:
            pass
            # b

        # a
        # c

else:
    pass

# Same indent

if True:
    if True:
        if True:
            pass

        #a
        #b
        #c
else:
    pass

if True:
    if True:
        if True:
            pass

        # a
        # b
        # c

else:
    pass


# Regression test for https://github.com/astral-sh/ruff/issues/5337
if parent_body:
    if current_body:
        child_in_body()
        last_child_in_current_body()  # may or may not have children on its own
# a
  # b
    # c
      # d
        # e
          #f
