# There are trailing whitespace before the newline character but those whitespaces are
# part of the comment token.
# https://github.com/astral-sh/ruff/issues/11929

f"""hello {x # comment    
y = 1