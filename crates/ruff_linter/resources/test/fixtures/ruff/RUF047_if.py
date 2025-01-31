### Errors

if False:
    condition_is_not_evaluated()
else:
    pass


if this_comment():
    belongs_to()  # `if`
else:
    ...


if elif_is():
    treated()
elif the_same():
    as_if()
else:
    pass


if this_second_comment():
    belongs()  # to
      # `if`
else:
    pass

if this_second_comment():
    belongs()  # to
    # `if`
else:
    pass


if of_course():
    this()
else:
    ...
# this comment doesn't belong to the if


if of_course: this()
else: ...


if of_course:
    this() # comment
else: ...


def nested():
    if a:
        b()
    else:
        ...


### No errors


if this_second_comment():
    belongs()  # to
# `else`
else:
    pass


if of_course():
    this()
else:
    ...  # too


if of_course():
    this()
else:
    ...
    # comment


if of_course:
    this() # comment
else: ...  # trailing
