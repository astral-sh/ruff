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


#############################################################
# Assignments where the target or annotations are splittable
#############################################################


# The target splits because of a magic trailing comma
# The string is joined and not parenthesized because it just fits into the line length (including comment).
a[
    aaaaaaa,
    b,
] = "ccccccccccccccccccccccccccccc" "cccccccccccccccccccccccccccccccccccccccccc"  # comment

# Same but starting with a joined string. They should both result in the same formatting.
[
    aaaaaaa,
    b,
] = "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"  # comment

# The target splits because of the magic trailing comma
# The string is **not** joined because it with the inlined comment exceeds the line length limit.
a[
    aaaaaaa,
    b,
] = "ccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccccccccccccccc"  # comment


# The target should be flat
# The string should be joined because it fits into the line length
a[
    aaaaaaa,
    b
] = (
    "ccccccccccccccccccccccccccccccccccc" "cccccccccccccccccccccccc"  # comment
)

# Same but starting with a joined string. They should both result in the same formatting.
a[
    aaaaaaa,
    b
] = "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"  # comment

# The target should be flat
# The string gets parenthesized because it, with the inlined comment, exceeds the line length limit.
a[
    aaaaaaa,
    b
] = "ccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccccccccccccccc"  # comment


# Split an overlong target, but join the string if it fits
a[
    aaaaaaa,
    b
].bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = (
    "ccccccccccccccccccccccccccccccccccccccccc" "cccccccccccccccccccccccccccccc"  # comment
)

# Split both if necessary and keep multiline
a[
    aaaaaaa,
    b
].bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = (
    "ccccccccccccccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccc"  # comment
)

#########################################################
# Leading or trailing own line comments:
# Preserve the parentheses
########################################################
a[
    aaaaaaa,
    b
] = (
    # test
    "ccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccccccccccccccc"
)

a[
    aaaaaaa,
    b
] = (
    "ccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccccccccccccccc"
    # test
)

a[
    aaaaaaa,
    b
] = (
    "ccccccccccccccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccccccccccccccc"
    # test
)


#############################################################
# Type alias statements
#############################################################

# First break the right, join the string
type A[str, int, number] = "Literal[string, int] | None | " "CustomType" "| OtherCustomTypeExcee"  # comment

# Keep multiline if overlong
type A[str, int, number] = "Literal[string, int] | None | " "CustomTypeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"  # comment

# Break the left if it is over-long, join the string
type Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[stringgggggggggg, inttttttttttttttttttttttt, number] = "Literal[string, int] | None | " "CustomType"  # comment

# Break both if necessary and keep multiline
type Aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[stringgggggggggg, inttttttttttttttttttttttt, number] = "Literal[string, int] | None | " "CustomTypeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"  # comment


#############################################################
# F-Strings
#############################################################

# Flatten and join the f-string
aaaaaaaaaaa = f"test{
expression}flat" f"cean beeeeeeee {joined} eeeeeeeeeeeeeeeee" # inline

# Parenthesize the value and join it, inline the comment
aaaaaaaaaaa = f"test{
expression}flat" f"cean beeeeeeee {joined} eeeeeeeeeeeeeeeeeeeeeeeeeee" # inline

# Parenthesize the f-string and keep it multiline because it doesn't fit on a single line including the comment
aaaaaaaaaaa = f"test{
expression
}flat" f"cean beeeeeeee {
joined
} eeeeeeeeeeeeeeeeeeeeeeeeeeeee" # inline


# The target splits because of a magic trailing comma
# The string is joined and not parenthesized because it just fits into the line length (including comment).
a[
    aaaaaaa,
    b,
] = f"ccccc{
expression}ccccccccccc" f"cccccccccccccccccccccccccccccccccccccccccc"  # comment


# Same but starting with a joined string. They should both result in the same formatting.
[
    aaaaaaa,
    b,
] = f"ccccc{
expression}ccccccccccccccccccccccccccccccccccccccccccccccccccccc"  # comment

# The target splits because of the magic trailing comma
# The string is **not** joined because it with the inlined comment exceeds the line length limit.
a[
    aaaaaaa,
    b,
] = f"ccccc{
expression}cccccccccccccccccccc" f"cccccccccccccccccccccccccccccccccccccccccc"  # comment


# The target should be flat
# The string should be joined because it fits into the line length
a[
    aaaaaaa,
    b
] = (
    f"ccccc{
    expression}ccccccccccc" "cccccccccccccccccccccccc"  # comment
)

# Same but starting with a joined string. They should both result in the same formatting.
a[
    aaaaaaa,
    b
] = f"ccccc{
expression}ccccccccccccccccccccccccccccccccccc"  # comment

# The target should be flat
# The string gets parenthesized because it, with the inlined comment, exceeds the line length limit.
a[
    aaaaaaa,
    b
] = f"ccccc{
expression}ccccccccccc" "ccccccccccccccccccccccccccccccccccccccccccc"  # comment


# Split an overlong target, but join the string if it fits
a[
    aaaaaaa,
    b
].bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = (
    f"ccccc{
    expression}ccccccccccc" "cccccccccccccccccccccccccccccc"  # comment
)

# Split both if necessary and keep multiline
a[
    aaaaaaa,
    b
].bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = (
    f"ccccc{
    expression}cccccccccccccccccccccccccccccccc" "ccccccccccccccccccccccccccccccc"  # comment
)

# Don't inline f-strings that contain expressions that are guaranteed to split, e.b. because of a magic trailing comma
aaaaaaaaaaaaaaaaaa = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}" "moreeeeeeeeeeeeeeeeeeee" "test" # comment

aaaaaaaaaaaaaaaaaa = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}" "moreeeeeeeeeeeeeeeeeeee" "test" # comment
)

aaaaa[aaaaaaaaaaa] = f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}" "moreeeeeeeeeeeeeeeeeeee" "test" # comment

aaaaa[aaaaaaaaaaa] = (f"testeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee{
[a,]
}" "moreeeeeeeeeeeeeeeeeeee" "test" # comment
)

# Don't inline f-strings that contain commented expressions
aaaaaaaaaaaaaaaaaa = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{[
        a  # comment
    ]}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaa[aaaaaaaaaaa] = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{[
        a  # comment
    ]}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)

# Don't inline f-strings with multiline debug expressions:
aaaaaaaaaaaaaaaaaa = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{
    a=}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaaaaaaaaaaaaaaa = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{a +
    b=}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaaaaaaaaaaaaaaa = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{a
    =}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaa[aaaaaaaaaaa] = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{
    a=}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)

aaaaa[aaaaaaaaaaa] = (
    f"testeeeeeeeeeeeeeeeeeeeeeeeee{a
    =}" "moreeeeeeeeeeeeeeeeeetest"  # comment
)


# Trailing last-part comments

a = (
    "a"
    "b"  # belongs to `b`
)

a: Literal[str] = (
    "a"
    "b"  # belongs to `b`
)

a += (
    "a"
    "b"  # belongs to `b`
)

a = (
    r"a"
    r"b"  # belongs to `b`
)

a = (
    "a"
    "b"
)  # belongs to the assignment

a = (((
    "a"
    "b"  # belongs to `b`
)))

a = (((
    "a"
    "b"
) # belongs to the f-string expression
))

a = (
    "a" "b"  # belongs to the f-string expression
)

a = (
    "a" "b"
    # belongs to the f-string expression
)

# There's no "right" answer if some parts are on the same line while others are on separate lines.
# This is likely a comment for one of the last two parts but could also just be a comment for the entire f-string expression.
# Because there's no right answer, follow what we do elsewhere and associate the comment with the outer-most node which
# is the f-string expression.
a = (
    "a"
    "b" "ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"  # belongs to the f-string expression
)

logger.error(
    f"Failed to run task {task} for job"  
    f"with id {str(job.id)}" # type: ignore[union-attr]
)

a = (10 +
     "Exception in {call_back_name} "
     f"'{msg}'"  # belongs to binary operation
)

a = 10 + (
    "Exception in {call_back_name} "
    f"'{msg}'"  # belongs to f-string
)

self._attr_unique_id = (
    f"{self._device.temperature.group_address_state}_"
    f"{self._device.target_temperature.group_address_state}_"
    f"{self._device.target_temperature.group_address}_"
    f"{self._device._setpoint_shift.group_address}"  # noqa: SLF001
)

return (
    f"Exception in {call_back_name} when handling msg on "
    f"'{msg.topic}': '{msg.payload}'"  # type: ignore[str-bytes-safe]
)