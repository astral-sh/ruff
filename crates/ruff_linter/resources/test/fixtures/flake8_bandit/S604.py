def foo(shell):
    pass


foo(shell=True)

foo(shell={**{}})
foo(shell={**{**{}}})

# Truthy non-bool values for `shell`
foo(shell=(i for i in ()))
foo(shell=lambda: 0)

# f-strings guaranteed non-empty
foo(shell=f"{b''}")
x = 1
foo(shell=f"{x=}")

# Additional truthiness cases for generator, lambda, and f-strings
foo(shell=(i for i in ()))
foo(shell=lambda: 0)
foo(shell=f"{b''}")
x = 1
foo(shell=f"{x=}")
