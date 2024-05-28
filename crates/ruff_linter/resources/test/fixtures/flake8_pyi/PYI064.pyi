from typing import Final, Literal

x: Final[Literal[True]]  # PYI064
y: Final[Literal[None]] = None  # PYI064
z: Final[Literal["this is a really long literal, that won't be rendered in the issue text"]]  # PYI064

# This should be fixable, and marked as safe
w1: Final[Literal[123]]  # PYI064

# This should not be fixable
w2: Final[Literal[123]] = "random value"  # PYI064

n1: Final[Literal[True, False]] # No issue here
n2: Literal[True]  # No issue here

PlatformName = Literal["linux", "macos", "windows"]
PLATFORMS: Final[set[PlatformName]] = {"linux", "macos", "windows"}  # No issue here

foo: Final[{1, 2, 3}] = {1, 2, 3}  # No issue here
