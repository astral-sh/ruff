try:
    raise try_body_is_not_checked()
except:
    pass
else:
    pass


try:
    this()
except comment:
    belongs()
except:
    to()  # `except`
else:
    ...


try:
    this()
except (second, comment):
    belongs()  # to
# `else`
else:
    pass
    ...


try:
    and_so()
except:
    does()
# this
else:
    ...
    pass


try:
    of_course()
except:
    this()
else:
    ...  # too
