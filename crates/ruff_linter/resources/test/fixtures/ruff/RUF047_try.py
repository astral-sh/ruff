### Errors

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
    of_course()
except:
    this()
else:
    ...
# This comment belongs to finally
finally:
    pass


try:
    of_course()
except:
    this()
else:
    ...
# This comment belongs to the statement coming after the else


### No errors

try:
    this()
except (second, comment):
    belongs()  # to
# `else`
else:
    pass


try:
    and_so()
except:
    does()
# this
else:
    ...


try:
    of_course()
except:
    this()
else:
    ...  # too

try:
    of_course()
except:
    this()
else:
    ...
    # This comment belongs to else
finally:
    pass
