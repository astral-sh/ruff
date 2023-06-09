# Errors.
foo == foo

foo != foo

foo > foo

foo >= foo

foo < foo

foo <= foo

foo is foo

foo is not foo

foo in foo

foo not in foo

# Non-errors.
"foo" == "foo"  # This is flagged by `comparison-of-constant` instead.

foo == "foo"

foo == bar

foo != bar

foo > bar

foo >= bar

foo < bar

foo <= bar

foo is bar

foo is not bar

foo in bar

foo not in bar
