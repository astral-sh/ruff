obj = {}

key in obj.keys()  # SIM118

key not in obj.keys()  # SIM118

foo["bar"] in obj.keys()  # SIM118

foo["bar"] not in obj.keys()  # SIM118

foo['bar'] in obj.keys()  # SIM118

foo['bar'] not in obj.keys()  # SIM118

foo() in obj.keys()  # SIM118

foo() not in obj.keys()  # SIM118

for key in obj.keys():  # SIM118
    pass

for key in list(obj.keys()):
    if some_property(key):
        del obj[key]

[k for k in obj.keys()]  # SIM118

{k for k in obj.keys()}  # SIM118

{k: k for k in obj.keys()}  # SIM118

(k for k in obj.keys())  # SIM118

key in (obj or {}).keys()  # SIM118

(key) in (obj or {}).keys()  # SIM118

from typing import KeysView


class Foo:
    def keys(self) -> KeysView[object]:
        ...

    def __contains__(self, key: object) -> bool:
        return key in self.keys()  # OK


# Regression test for: https://github.com/astral-sh/ruff/issues/7124
key in obj.keys()and foo
(key in obj.keys())and foo
key in (obj.keys())and foo

# Regression test for: https://github.com/astral-sh/ruff/issues/7200
for key in (
    self.experiment.surveys[0]
        .stations[0]
        .keys()
):
    continue

from builtins import dict as SneakyDict

d = SneakyDict()
key in d.keys()  # SIM118
