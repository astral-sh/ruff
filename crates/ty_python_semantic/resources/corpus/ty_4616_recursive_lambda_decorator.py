# Regression test for https://github.com/astral-sh/ty/issues/4616

(make := lambda: result)
(alias := make)
try:
    first
except* 0:
    @make or fallback
    class result:
        pass

try:
    second
except* 0:
    (result := alias)
finally:
    from unknown_module import member as result
