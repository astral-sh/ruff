class TestClass:
    def __bool__(self):
        ...

    def __bool__(self, x):  # too many mandatory args
        ...

    def __bool__(self, x=1):  # additional optional args OK
        ...

    def __bool__():  # ignored; should be caughty by E0211/N805
        ...

    @staticmethod
    def __bool__():
        ...

    @staticmethod
    def __bool__(x):  # too many mandatory args
        ...

    @staticmethod
    def __bool__(x=1):  # additional optional args OK
        ...

    def __eq__(self, other):  # multiple args
        ...

    def __eq__(self, other=1):  # expected arg is optional
        ...

    def __eq__(self):  # too few mandatory args
        ...

    def __eq__(self, other, other_other):  # too many mandatory args
        ...

    def __round__(self):  # allow zero additional args
        ...

    def __round__(self, x):  # allow one additional args
        ...

    def __round__(self, x, y):  # disallow 2 args
        ...

    def __round__(self, x, y, z=2):  # disallow 3 args even when one is optional
        ...

    def __eq__(self, *args):  # ignore *args
        ...

    def __eq__(self, x, *args):  # extra *args is ok
        ...

    def __eq__(self, x, y, *args):  # too many args with *args
        ...

    def __round__(self, *args):  # allow zero additional args
        ...

    def __round__(self, x, *args):  # allow one additional args
        ...

    def __round__(self, x, y, *args):  # disallow 2 args
        ...

    def __eq__(self, **kwargs):  # ignore **kwargs
        ...

    def __eq__(self, /, other=42):  # support positional-only args
        ...

    def __eq__(self, *, other=42):  # support positional-only args
        ...

    def __cmp__(self): # #16217 assert non-special method is skipped, expects 2 parameters
        ...

    def __div__(self): # #16217 assert non-special method is skipped, expects 2 parameters
        ...

    def __nonzero__(self, x): # #16217 assert non-special method is skipped, expects 1 parameter
        ...

    def __unicode__(self, x): # #16217 assert non-special method is skipped, expects 1 parameter
        ...

    def __next__(self, x): # #16217 assert special method is linted, expects 1 parameter
        ...

    def __buffer__(self): # #16217 assert special method is linted, expects 2 parameters
        ...

    def __class_getitem__(self): # #16217 assert special method is linted, expects 2 parameters
        ...

    def __mro_entries__(self): # #16217 assert special method is linted, expects 2 parameters
        ...

    def __release_buffer__(self): # #16217 assert special method is linted, expects 2 parameters
        ...

    def __subclasshook__(self): # #16217 assert special method is linted, expects 2 parameters
        ...

    def __setattr__(self, /, name): # #16217 assert positional-only special method is linted, expects 3 parameters
        ...

    def __setitem__(self, key, /, value, extra_value): # #16217 assert positional-only special method is linted, expects 3 parameters
        ...