send(
    request,
    translate("Hello, %(name)s") % {
        "name": display_name,
    },
)
message = "%s" % {
    "name": display_name,
}
configure(
    defaults | {
        "first": first_value,
        "second": second_value,
    },
)
message = "%s" % {key: value for key, value in pairs}

# Long prefixes and comments still need line breaks before the operator.
message = very_long_left_hand_function_name(argument) % {
    "key": value,
}
message = (
    template  # comment on the left operand
    % {"key": value,}
)
message = template % {
    # comment in the dictionary
    "key": value,
}

# Multiple operators retain their normal precedence and wrapping.
message = template % {"key": value,} % {"key": value,}
message = template % (first, second,)
