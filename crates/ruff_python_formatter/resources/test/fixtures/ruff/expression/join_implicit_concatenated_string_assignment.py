## Implicit concatenated strings with a trailing comment but a non splittable target.

# Don't join the string because the joined string with the inlined comment exceeds the line length limit.
____aaa = (
    "aaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvv"
)  # c

# This is the same string as above and should lead to the same formatting. The only difference is that we start
# with an unparenthesized string.
____aaa = "aaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvv"  # c

# Again the same string as above but this time as non-implicit concatenated string.
# It's okay if the formatting differs because it's an explicit choice to use implicit concatenation.
____aaa = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvv"  # c

# Join the string because it's exactly in the line length limit when the comment is inlined.
____aaa = (
    "aaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvv"
)  # c

# This is the same string as above and should lead to the same formatting. The only difference is that we start
# with an unparenthesized string.
____aaa = "aaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvv"  # c

# Again the same string as above but as a non-implicit concatenated string. It should result in the same formatting
# (for consistency).
____aaa = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvv"  # c

# It should collapse the parentheses if the joined string and the comment fit on the same line.
# This is required for stability.
____aaa = (
    "aaaaaaaaaaaaaaaaaaaaa" "aaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvv"  # c
)
