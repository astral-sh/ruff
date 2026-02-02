"{}".format(1, bar=2)  # F522
"{bar}{}".format(1, bar=2, spam=3)  # F522
"{bar:{spam}}".format(bar=2, spam=3)  # No issues
"{bar:{spam}}".format(bar=2, spam=3, eggs=4, ham=5)  # F522
(''
 .format(x=2))  # F522

# https://github.com/astral-sh/ruff/issues/18806
# The fix here is unsafe because the unused argument has side effect
"Hello, {name}".format(greeting=print(1), name="World")

# The fix here is safe because the unused argument has no side effect,
# even though the used argument has a side effect
"Hello, {name}".format(greeting="Pikachu", name=print(1))
