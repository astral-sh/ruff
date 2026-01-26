# ref: https://github.com/astral-sh/ty/issues/1262
raise NotImplement<CURSOR: NotImplementedError>

raise AssertionError from NotImplement<CURSOR: NotImplementedError>

try:
    raise AssertionError("invalid")
except NotImplement<CURSOR: NotImplementedError>
