# This file doesn't use any Python 3.9+ only grammars.


# Make sure parens around a single context manager don't get autodetected as
# Python 3.9+.
with (a):
    pass


with \
     make_context_manager1() as cm1, \
     make_context_manager2() as cm2, \
     make_context_manager3() as cm3, \
     make_context_manager4() as cm4 \
:
    pass


# output
# This file doesn't use any Python 3.9+ only grammars.


# Make sure parens around a single context manager don't get autodetected as
# Python 3.9+.
with a:
    pass


with make_context_manager1() as cm1, make_context_manager2() as cm2, make_context_manager3() as cm3, make_context_manager4() as cm4:
    pass
