"{".format(1)
"}".format(1)
"{foo[}".format(foo=1)
# too much string recursion (placeholder-in-placeholder)
"{:{:{}}}".format(1, 2, 3)
# ruff picks these issues up, but flake8 doesn't
"{foo[]}".format(foo={"": 1})
"{foo..}".format(foo=1)
"{foo..bar}".format(foo=1)

# The following are all "good" uses of .format
"{.__class__}".format("")
"{foo[bar]}".format(foo={"bar": "barv"})
"{[bar]}".format({"bar": "barv"})
"{:{}} {}".format(1, 15, 2)
"{:2}".format(1)
"{foo}-{}".format(1, foo=2)
a = ()
"{}".format(*a)
k = {}
"{foo}".format(**k)
