inferred_int = 1
inferred_float = 1.



round(42)                                         # Error (safe)
round(42, None)                                   # Error (safe)
round(42, 2)                                      # Error (safe)
round(42, -2)                                     # No error
round(42, inferred_int)                           # No error
round(42, 3 + 4)                                  # No error
round(42, foo)                                    # No error


round(42.)                                        # No error
round(42., None)                                  # No error
round(42., 2)                                     # No error
round(42., -2)                                    # No error
round(42., inferred_int)                          # No error
round(42., 3 + 4)                                 # No error
round(42., foo)                                   # No error


round(4 + 2)                                      # Error (safe)
round(4 + 2, None)                                # Error (safe)
round(4 + 2, 2)                                   # Error (safe)
round(4 + 2, -2)                                  # No error
round(4 + 2, inferred_int)                        # No error
round(4 + 2, 3 + 4)                               # No error
round(4 + 2, foo)                                 # No error


round(4. + 2.)                                    # No error
round(4. + 2., None)                              # No error
round(4. + 2., 2)                                 # No error
round(4. + 2., -2)                                # No error
round(4. + 2., inferred_int)                      # No error
round(4. + 2., 3 + 4)                             # No error
round(4. + 2., foo)                               # No error


round(inferred_int)                               # Error (unsafe)
round(inferred_int, None)                         # Error (unsafe)
round(inferred_int, 2)                            # Error (unsafe)
round(inferred_int, -2)                           # No error
round(inferred_int, inferred_int)                 # No error
round(inferred_int, 3 + 4)                        # No error
round(inferred_int, foo)                          # No error


round(inferred_float)                             # No error
round(inferred_float, None)                       # No error
round(inferred_float, 2)                          # No error
round(inferred_float, -2)                         # No error
round(inferred_float, inferred_int)               # No error
round(inferred_float, 3 + 4)                      # No error
round(inferred_float, foo)                        # No error


round(lorem)                                      # No error
round(lorem, None)                                # No error
round(lorem, 2)                                   # No error
round(lorem, -2)                                  # No error
round(lorem, inferred_int)                        # No error
round(lorem, 3 + 4)                               # No error
round(lorem, foo)                                 # No error

# Fixes should preserve parentheses when argument
# contains newline.
# See https://github.com/astral-sh/ruff/issues/15598
round(-
1)
round(1
*1
)

# fix should be unsafe if comment is in call range
round(# a comment
17
)
round(
    17 # a comment
)

# See: https://github.com/astral-sh/ruff/issues/21209
print(round(125, **{"ndigits": -2}))
print(round(125, *[-2]))