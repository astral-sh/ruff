"{".format(1)
# too much string recursion (placeholder-in-placeholder)
"{:{:{}}}".format(1, 2, 3)

# "{} {1}".format(1, 2)  # F525
# "{0} {}".format(1, 2)  # F525
# "{}".format(1, 2)  # F523
# "{}".format(1, bar=2)  # F522
# "{} {}".format(1)  # F524
# "{2}".format()  # F524
# "{bar}".format()  # F524

# The following are all "good" uses of .format
"{.__class__}".format("")
"{foo[bar]}".format(foo={"bar": "barv"})
"{:{}} {}".format(1, 15, 2)
"{:2}".format(1)
"{foo}-{}".format(1, foo=2)
a = ()
"{}".format(*a)
k = {}
"{foo}".format(**k)
