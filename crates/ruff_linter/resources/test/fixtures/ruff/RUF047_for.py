### Errors

for _ in range(0):
    loop_body_is_not_checked()
    break
else:
    pass


for this in comment:
    belongs_to()  # `for`
else:
    ...


for of in course():
    this()
else:
    ...
# this comment does not belong to the else


### No errors

for this in second_comment:
    belongs()  # to
# `else`
else:
    pass


for _and in so:
    does()
# this
else:
    pass


for of in course():
    this()
else:
    ...  # too


for of in course():
    this()
else:
    ...
    # too
