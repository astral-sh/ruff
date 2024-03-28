this_should_be_linted = 'single quote string'
this_should_be_linted = u'double quote string'
this_should_be_linted = f'double quote string'
this_should_be_linted = f'double {"quote"} string'

# https://github.com/astral-sh/ruff/issues/10546
x: "Literal['foo', 'bar']"
