lazy import foo
lazy import package.submodule
lazy import aliased as alias
lazy from library import Component
lazy from library import Other as Renamed
lazy import multi_one, multi_two

base = foo.Base
component = Component
aliased = alias.Member
submodule = package.submodule.Member
multi_value = multi_one.Value

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

list_values = [foo.Value for _ in range(1)]
set_values = {Component for _ in range(1)}
dict_values = {foo.Key: Component for _ in range(1)}
generator_values = (foo.Value for _ in range(1))

# TODO(charlie): Avoid flagging references behind expression-level control flow.
short_circuit = False and foo.Value
conditional_expression = foo.Value if enabled else None

if TYPE_CHECKING:
    type_checking_only = foo.Value
