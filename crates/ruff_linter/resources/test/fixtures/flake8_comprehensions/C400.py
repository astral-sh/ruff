# Cannot combine with C416. Should use list comprehension here.
even_nums = list(2 * x for x in range(3))
odd_nums = list(
    2 * x + 1 for x in range(3)
)


# Short-circuit case, combine with C416 and should produce x = list(range(3))
x = list(x for x in range(3))
x = list(
    x for x in range(3)
)

# Strip parentheses from inner generators.
list((2 * x for x in range(3)))
list(((2 * x for x in range(3))))
list((((2 * x for x in range(3)))))

# Not built-in list.
def list(*args, **kwargs):
    return None


list(2 * x for x in range(3))
list(x for x in range(3))
