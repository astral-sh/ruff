_("{}".format("line"))
gettext("{}".format("line"))
ngettext("{}".format("line"), "{}".format("lines"), 2)

_gettext("{}".format("line"))  # no lint
gettext_fn("{}".format("line"))  # no lint


# https://github.com/astral-sh/ruff/issues/19028
import gettext as gettext_mod
from gettext import (
    gettext as gettext_fn,
    ngettext as ngettext_fn,
)

name = "Guido"

gettext_mod.gettext("Hello, {}!".format(name))
gettext_mod.ngettext("Hello, {}!".format(name), "Hello, {}s!".format(name), 2)
gettext_fn("Hello, {}!".format(name))
ngettext_fn("Hello, {}!".format(name), "Hello, {}s!".format(name), 2)


# https://github.com/astral-sh/ruff/issues/19028 - deferred translations
def _(message): return message

animals = [_("{}".format("mollusk")),
           _("{}".format("albatross")),
           _("{}".format("rat")),
           _("{}".format("penguin")),
           _("{}".format("python")),]

del _

for a in animals:
    print(_(a))
    print(_("{}".format(a)))


# https://github.com/astral-sh/ruff/issues/19028
#  - installed translations/i18n functions dynamically assigned to builtins
import builtins

gettext_mod.install("domain")
builtins.__dict__["gettext"] = gettext_fn

builtins._("{}".format("line"))
builtins.gettext("{}".format("line"))
builtins.ngettext("{}".format("line"), "{}".format("lines"), 2)
