"""Record of phased-in incompatible language changes.

Each line is of the form:

    FeatureName = "_Feature(" OptionalRelease "," MandatoryRelease ","
                              CompilerFlag ")"

where, normally, OptionalRelease < MandatoryRelease, and both are 5-tuples
of the same form as sys.version_info:

    (PY_MAJOR_VERSION, # the 2 in 2.1.0a3; an int
     PY_MINOR_VERSION, # the 1; an int
     PY_MICRO_VERSION, # the 0; an int
     PY_RELEASE_LEVEL, # "alpha", "beta", "candidate" or "final"; string
     PY_RELEASE_SERIAL # the 3; an int
    )

OptionalRelease records the first release in which

    from __future__ import FeatureName

was accepted.

In the case of MandatoryReleases that have not yet occurred,
MandatoryRelease predicts the release in which the feature will become part
of the language.

Else MandatoryRelease records when the feature became part of the language;
in releases at or after that, modules no longer need

    from __future__ import FeatureName

to use the feature in question, but may continue to use such imports.

MandatoryRelease may also be None, meaning that a planned feature got
dropped or that the release version is undetermined.

Instances of class _Feature have two corresponding methods,
.getOptionalRelease() and .getMandatoryRelease().

CompilerFlag is the (bitfield) flag that should be passed in the fourth
argument to the builtin function compile() to enable the feature in
dynamically compiled code.  This flag is stored in the .compiler_flag
attribute on _Future instances.  These values must match the appropriate
#defines of CO_xxx flags in Include/cpython/compile.h.

No feature line is ever to be deleted from this file.
"""

from typing_extensions import TypeAlias

_VersionInfo: TypeAlias = tuple[int, int, int, str, int]

class _Feature:
    def __init__(self, optionalRelease: _VersionInfo, mandatoryRelease: _VersionInfo | None, compiler_flag: int) -> None: ...
    def getOptionalRelease(self) -> _VersionInfo:
        """Return first release in which this feature was recognized.

        This is a 5-tuple, of the same form as sys.version_info.
        """

    def getMandatoryRelease(self) -> _VersionInfo | None:
        """Return release in which this feature will become mandatory.

        This is a 5-tuple, of the same form as sys.version_info, or, if
        the feature was dropped, or the release date is undetermined, is None.
        """
    compiler_flag: int

absolute_import: _Feature
division: _Feature
generators: _Feature
nested_scopes: _Feature
print_function: _Feature
unicode_literals: _Feature
with_statement: _Feature
barry_as_FLUFL: _Feature
generator_stop: _Feature
annotations: _Feature

all_feature_names: list[str]  # undocumented

__all__ = [
    "all_feature_names",
    "absolute_import",
    "division",
    "generators",
    "nested_scopes",
    "print_function",
    "unicode_literals",
    "with_statement",
    "barry_as_FLUFL",
    "generator_stop",
    "annotations",
]
