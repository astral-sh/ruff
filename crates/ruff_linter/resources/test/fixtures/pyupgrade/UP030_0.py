# Invalid calls; errors expected.

"{0}" "{1}" "{2}".format(1, 2, 3)

"a {3} complicated {1} string with {0} {2}".format(
    "first", "second", "third", "fourth"
)

'{0}'.format(1)

'{0:x}'.format(30)

x = '{0}'.format(1)

'''{0}\n{1}\n'''.format(1, 2)

x = "foo {0}" \
    "bar {1}".format(1, 2)

("{0}").format(1)

"\N{snowman} {0}".format(1)

print(
    'foo{0}'
    'bar{1}'.format(1, 2)
)

print(
    'foo{0}'  # ohai\n"
    'bar{1}'.format(1, 2)
)

'{' '0}'.format(1)

args = list(range(10))
kwargs = {x: x for x in range(10)}

"{0}".format(*args)

"{0}".format(**kwargs)

"{0}_{1}".format(*args)

"{0}_{1}".format(1, *args)

"{0}_{1}".format(1, 2, *args)

"{0}_{1}".format(*args, 1, 2)

"{0}_{1}_{2}".format(1, **kwargs)

"{0}_{1}_{2}".format(1, 2, **kwargs)

"{0}_{1}_{2}".format(1, 2, 3, **kwargs)

"{0}_{1}_{2}".format(1, 2, 3, *args, **kwargs)

"{1}_{0}".format(1, 2, *args)

"{1}_{0}".format(1, 2)
