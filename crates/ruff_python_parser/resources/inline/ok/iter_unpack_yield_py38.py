# parse_options: {"target-version": "3.8"}
rest = (4, 5, 6)
def g(): yield 1, 2, 3, *rest
