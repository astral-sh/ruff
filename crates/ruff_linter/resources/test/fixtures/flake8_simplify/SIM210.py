a = True if b else False  # SIM210

a = True if b != c else False  # SIM210

a = True if b + c else False  # SIM210

a = False if b else True  # OK


def f():
    # OK
    def bool():
        return False

    a = True if b else False


# Regression test for: https://github.com/astral-sh/ruff/issues/7076
samesld = True if (psl.privatesuffix(urlparse(response.url).netloc) ==
                                   psl.privatesuffix(src.netloc)) else False
