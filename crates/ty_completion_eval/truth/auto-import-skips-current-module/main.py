Kadabra = 1

# This is meant to reflect that auto-import
# does *not* include completions for `Kadabra`.
# That is, before a bug was fixed, completions
# would offer two variants for `Kadabra`: one
# for the current module (correct) and another
# from auto-import that would insert
# `from main import Kadabra` into this module
# (incorrect).
#
# Since the incorrect one wasn't ranked above
# the correct one, this task unfortunately
# doesn't change the evaluation results. But
# I've added it anyway in case it does in the
# future (or if we change our evaluation metric
# to something that incorporates suggestions
# after the correct one).
Kada<CURSOR: Kadabra>
