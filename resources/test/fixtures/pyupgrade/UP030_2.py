# These SHOULD change

args = list(range(10))
kwargs = {x: x for x in range(10)}

"{0}".format(*args)

"{0}".format(**kwargs)

"{0}_{1}".format(*args)

"{0}_{1}".format(1, *args)

"{1}_{0}".format(*args)

"{1}_{0}".format(1, *args)

"{0}_{1}".format(1, 2, *args)

"{0}_{1}".format(*args, 1, 2)

"{0}_{1}_{2}".format(1, **kwargs)

"{0}_{1}_{2}".format(1, 2, **kwargs)

"{0}_{1}_{2}".format(1, 2, 3, **kwargs)

"{0}_{1}_{2}".format(1, 2, 3, *args, **kwargs)
