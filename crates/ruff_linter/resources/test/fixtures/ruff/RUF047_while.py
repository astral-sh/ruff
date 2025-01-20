### No errors

while True:
    loop_body_is_not_checked()
    break
else:
    pass


while this_comment:
    belongs_to()  # `for`
else:
    ...


while of_course():
    this()
else:
    ...
# this comment belongs to the statement coming after the else


### No errors

while this_second_comment:
    belongs()  # to
# `else`
else:
    pass


while and_so:
    does()
# this
else:
    ...


while of_course():
    this()
else:
    ...  # too

while of_course():
    this()
else:
    ...
    # this comment belongs to the else

