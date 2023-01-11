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

'{' '0}'.format(1)

# These will not change because we are waiting for libcst to fix this issue:
# https://github.com/Instagram/LibCST/issues/846
print(
    'foo{0}'
    'bar{1}'.format(1, 2)
)

print(
    'foo{0}'  # ohai\n"
    'bar{1}'.format(1, 2)
)
