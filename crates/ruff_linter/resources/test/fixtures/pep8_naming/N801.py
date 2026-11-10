class bad:
    pass


class _bad:
    pass


class __bad:
    pass


class bad_class:
    pass


class Bad_Class:
    pass

class Bad__Class:
    pass

class BAD_CLASS:
    pass


class Good:
    pass


class _Good:
    pass


class GoodClass:
    pass


class GOOD:
    pass

class __GoodClass:
    pass


# A trailing underscore avoids a clash with a keyword (PEP 8), see #30230
class Good_:
    pass


class _Good_:
    pass


class __Good__:
    pass


class bad_:
    pass


class Bad_Class_:
    pass


class _:
    pass
