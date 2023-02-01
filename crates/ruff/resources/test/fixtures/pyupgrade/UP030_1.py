# Valid calls; no errors expected.

'{}'.format(1)


x = ('{0} {1}',)

'{0} {0}'.format(1)

'{0:<{1}}'.format(1, 4)

f"{0}".format(a)

f"{0}".format(1)

print(f"{0}".format(1))

# I did not include the following tests because ruff does not seem to work with
# invalid python syntax (which is a good thing)

# "{0}"format(1)
# '{'.format(1)", "'}'.format(1)
# ("{0}" # {1}\n"{2}").format(1, 2, 3)
