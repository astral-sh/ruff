lazy import foo
lazy import package.submodule
lazy import aliased as alias
lazy from library import Component
lazy from library import Other as Renamed

base = foo.Base
component = Component
aliased = alias.Member
submodule = package.submodule.Member

if foo.ENABLED:
    pass

if condition:
    conditional = foo.Value

try:
    try_body = Component
except Exception:
    handler = foo.Error


class Derived(foo.Base):
    component = Component

    if condition:
        conditional = Component

    def method(self):
        return foo.Value


def function(default=Renamed):
    return foo.Value


type Alias = foo.Type
