_(f"{'value'}")

# Don't trigger for t-strings
_(t"{'value'}")

gettext(f"{'value'}")
ngettext(f"{'value'}", f"{'values'}", 2)

_gettext(f"{'value'}")  # no lint
gettext_fn(f"{'value'}")  # no lint


# https://github.com/astral-sh/ruff/issues/19028 - import aliases
import gettext as gettext_mod
from gettext import (
    gettext as gettext_fn,
    ngettext as ngettext_fn,
)

name = "Guido"

gettext_mod.gettext(f"Hello, {name}!")
gettext_mod.ngettext(f"Hello, {name}!", f"Hello, {name}s!", 2)
gettext_fn(f"Hello, {name}!")
ngettext_fn(f"Hello, {name}!", f"Hello, {name}s!", 2)


# https://github.com/astral-sh/ruff/issues/19028 - deferred translations
def _(message): return message

animals = [_(f"{'mollusk'}"),
           _(f"{'albatross'}"),
           _(f"{'rat'}"),
           _(f"{'penguin'}"),
           _(f"{'python'}"),]

del _

for a in animals:
    print(_(a))
    print(_(f"{a}"))


# https://github.com/astral-sh/ruff/issues/19028
#  - installed translations/i18n functions dynamically assigned to builtins
import builtins

gettext_mod.install("domain")
builtins.__dict__["gettext"] = gettext_fn

builtins._(f"{'value'}")
builtins.gettext(f"{'value'}")
builtins.ngettext(f"{'value'}", f"{'values'}", 2)
