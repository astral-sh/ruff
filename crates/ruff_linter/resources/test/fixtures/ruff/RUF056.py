# These should trigger the rule, including fix suggestions
f"{a}"
f"{a!a}"
f"{a!r}"
f"{a!s}"

# These should not trigger the rule, because they have format specifications
f"{a:.3}"
f"{a:>}"
f"{a:+}"

# These shouldn't trigger the rule, since they don't consist entirely of a single replacement field
f"{a} "
f" {a}"

# Implicitly joined strings also shouldn't trigger the rule, since they're more complex to reason about (even though the behaviour is equivalent in this example) and not as likely to actually happen.
(f"{a}" "")
("" f"{a}")
