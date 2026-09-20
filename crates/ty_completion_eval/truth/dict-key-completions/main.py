from typing import Literal, TypedDict

from values import alias as imported_config


# The inferred key type is broad str, but initializer keys are useful suggestions.
config = {"host": "localhost", "port": 8000}
config["<CURSOR: host>"]

# An alias keeps the initializer that reached its assignment.
alias = config
config = {"timeout": 30}
alias["ho<CURSOR: host>"]
config["ti<CURSOR: timeout>"]

# Nested dictionary values and imported aliases retain initializer suggestions.
nested = {"database": {"host": "localhost", "port": 5432}}
nested["database"]["po<CURSOR: port>"]
imported_config["ex<CURSOR: exported>"]


def global_config() -> None:
    alias["ho<CURSOR: host>"]


def loop_config(flag: bool) -> None:
    current = {"initial": 1}
    while flag:
        current["la<CURSOR: later>"]
        current = {"later": 2}


# Reaching definitions exclude a statically unreachable reassignment.
reachable = {"current": 1}
if False:
    reachable = {"obsolete": 2}
reachable["cu<CURSOR: current>"]


# Finite expected string types remain available outside dictionary lookups.
def consume(value: Literal["declared"]) -> None:
    pass


consume("<CURSOR: declared>")


# TypedDict keys take precedence over the initializer.
class Settings(TypedDict):
    host: str
    port: int


settings: Settings = {"unexpected": 1}
settings["<CURSOR: host>"]
