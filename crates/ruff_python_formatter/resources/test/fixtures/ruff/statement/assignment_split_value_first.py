#######
# Unsplittable target and value

# Only parenthesize the value if it makes it fit, otherwise avoid parentheses.
b = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvee

bbbbbbbbbbbbbbbb = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvv

# Avoid parenthesizing the value even if the target exceeds the configured width
bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = bbb


############
# Splittable targets

# Does not double-parenthesize tuples
(
    first_item,
    second_item,
) = some_looooooooong_module.some_loooooog_function_name(
    first_argument, second_argument, third_argument
)


# Preserve parentheses around the first target
(
    req["ticket"]["steps"]["step"][0]["tasks"]["task"]["fields"]["field"][
        "access_request"
    ]["destinations"]["destination"][0]["ip_address"]
) = dst

# Augmented assignment
req["ticket"]["steps"]["step"][0]["tasks"]["task"]["fields"]["field"][
    "access_request"
] += dst

# Always parenthesize the value if it avoids splitting the target, regardless of the value's width.
_a: a[aaaa] = (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvv
)

#####
# Avoid parenthesizing the value if the expression right before the `=` splits to avoid an unnecessary pair of parentheses

# The type annotation is guaranteed to split because it is too long.
_a: a[
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvv
] = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvv

# The target is too long
(
    aaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb,
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvv

# The target splits because of a magic trailing comma
(
    a,
    b,
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvvvv

# The targets split because of a comment
(
    # leading
    a
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvvvv

(
    a
    # trailing
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvvvv

(
    a,  # nested
    b
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvvvv

#######
# Multi targets

# Black always parenthesizes the right if using multiple targets regardless if the parenthesized value exceeds the
# the configured line width or not
aaaa = bbbbbbbbbbbbbbbb = (
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvee
)

# Black does parenthesize the target if the target itself exceeds the line width and only parenthesizes
# the values if it makes it fit.
# The second target is too long to ever fit into the configured line width.
aaaa = (
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbdddd
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaabbbbbbbbbbbbbbbbbbbbbbbbbbbvvvvvvvvvvvvvvvvvee

# Does also apply for other multi target assignments, as soon as a single target exceeds the configured
# width
aaaaaa = a["aaa"] = bbbbb[aa, bbb, cccc] = dddddddddd = eeeeee = (
    fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff
) = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa


######################
# Call expressions:
# For unsplittable targets: Parenthesize the call expression if it makes it fit.
#
# For splittable targets:
# Only parenthesize a call expression if the parens of the call don't fit on the same line
# as the target. Don't parenthesize the call expression if the target (or annotation) right before
# splits.

# Don't parenthesize the function call if the left is unsplittable.
this_is_a_ridiculously_long_name_and_nobody_in_their_right_mind_would_use_one_like_it = a.b.function(
    arg1, arg2, arg3
)
this_is_a_ridiculously_long_name_and_nobody_in_their_right_mind_would_use_one_like_it = function(
    [1, 2, 3], arg1, [1, 2, 3], arg2, [1, 2, 3], arg3
)
this_is_a_ridiculously_long_name_and_nobody_in_their_right_mind_would_use_one_like_it = function(
    [1, 2, 3],
    arg1,
    [1, 2, 3],
    arg2,
    [1, 2, 3],
    arg3,
    dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd,
    eeeeeeeeeeeeee,
)

this_is_a_ridiculously_long_name_and_nobody_in_their_right_mind_would_use_one_like_it = (
    function()
)
this_is_a_ridiculously_long_name_and_nobodyddddddddddddddddddddddddddddddd = (
    a.b.function(arg1, arg2, arg3)
)
this_is_a_ridiculously_long_name_and_nobodyddddddddddddddddddddddddddddddd = function()
this_is_a_ridiculously_long_name_and_nobodyddddddddddddddddddddddddddddddd = function(
    [1, 2, 3], arg1, [1, 2, 3], arg2, [1, 2, 3], arg3
)
this_is_a_ridiculously_long_name_and_nobodyddddddddddddddddddddddddddddddd = function(
    [1, 2, 3],
    arg1,
    [1, 2, 3],
    arg2,
    [1, 2, 3],
    arg3,
    dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd,
    eeeeeeeeeeeeee,
)

####### Fluent call expressions
# Uses the regular `Multiline` layout where the entire `value` gets parenthesized
# if it doesn't fit on the line.
this_is_a_ridiculously_long_name_and_nobody_in_their_right_mind_would_use = (
    function().b().c([1, 2, 3], arg1, [1, 2, 3], arg2, [1, 2, 3], arg3)
)

#######
# Subscripts and non-fluent attribute chains
a = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[
    xxxxx
].bbvbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb[
    yyyyyyyyyy[aaaa]
] = ccccccccccccccccccccccccccccccccccc["aaaaaaa"]

a = aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa[
    xxxxx
].bbvbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb = ccccccccccccccccccccccccccccccccccc[
    "aaaaaaa"
]

label_thresholds[label_id] = label_quantiles[label_id][
    min(int(tolerance * num_thresholds), num_thresholds - 1)
]

#######
# Test comment inlining
value.__dict__[key] = (
    "test"  # set some Thrift field to non-None in the struct aa bb cc dd ee
)
value.__dict__.keye = (
    "test"  # set some Thrift field to non-None in the struct aa bb cc dd ee
)
value.__dict__.keye = (
    "test"  # set some Thrift field to non-None in the struct aa bb cc dd ee
)


# Don't parenthesize the value because the target's trailing comma forces it to split.
a[
    aaaaaaa,
    b,
] = cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  # comment

# Parenthesize the value, but don't duplicate the comment.
a[aaaaaaa, b] = (
    cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc  # comment
)

# Format both as flat, but don't loos the comment.
a[aaaaaaa, b] = bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  # comment

#######################################################
# Test the case where a parenthesized value now fits:
a[
    aaaaaaa,
    b
] = (
    cccccccc # comment
)

# Splits the target but not the value because of the magic trailing comma.
a[
    aaaaaaa,
    b,
] = (
    cccccccc # comment
)

# Splits the second target because of the comment and the first target because of the trailing comma.
a[
    aaaaaaa,
    b,
] = (
    # leading comment
    b
) = (
    cccccccc # comment
)


########
# Type Alias Statement
type A[str, int, number] = VeryLongTypeNameThatShouldBreakFirstToTheRightBeforeSplitngtin

type A[VeryLongTypeNameThatShouldBreakFirstToTheRightBeforeSplitngtinthatExceedsTheWidth] = str

