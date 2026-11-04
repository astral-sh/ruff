# Regression test for https://github.com/psf/black/issues/3442
# Dict keys with operators shouldn't break across multiple lines when the value
# can be wrapped onto a new line.

tests = [
    (
        BusinessHour(),
        {
            Timestamp("2014-07-04 15:00")
            + Nano(5): this_is_a_very_long_function("2014-07-04 16:00"),
            Timestamp("2014-07-04 16:00")
            + Nano(5): this_is_a_very_long_function("2014-07-04 16:00"),
            Timestamp("2014-07-04 16:00")
            - Nano(5): this_is_a_very_long_function("2014-07-04 16:00"),
        },
    ),
]

d1 = {
    Timestamp("2014-07-04 15:00") + Nano(5): a_very_long_variable * and_a_very_long_function_call() / 100000.0,
}

d2 = {
    "some_key" + "_suffix": "a very long string that exceeds the line length limit when placed with the key",
}

d3 = {
    key1 + key2: (
        first_long_variable_name
        + second_long_variable_name
        + third_long_variable_name
    ),
}

# Dict key that is genuinely too long by itself should still split inside the key:
tasks = {
    loop.run_in_executor(
        xx_xxxxxxxxxxxxxxxxx_xxxxx_xxxxxxx_xxxxxxxxxxxxxx_xxxxx_xxxxx
    ): src,
}

# Operations involving dictionaries (e.g. % string formatting) should also keep
# the operator and dict opening together when the dict value wraps:
self.message_user(
    request,
    gettext("Add another %(verbose_name)s")
    % {
        "verbose_name": capfirst(verbose_name),
    },
)
