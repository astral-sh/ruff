# Pandas has a boat load of test routines that we should never
# offer as completions (when Pandas is a dependency). Indeed, some
# of them (before we started filtering them out) get ranked above
# `TimeAmbiguous` for this particular query.
tambiguous<CURSOR: pandas.api.typing.aliases.TimeAmbiguous>

# We should include tests in our first party code.
zqzq<CURSOR: subdir.test_foo.test_zqzqzq>
