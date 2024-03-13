(aaaaaaaa
    + # trailing operator comment
    b # trailing right comment
)


(aaaaaaaa # trailing left comment
    +  # trailing operator comment
    # leading right comment
    b
)

(
    # leading left most comment
    aaaaaaaa
    +  # trailing operator comment
    # leading b comment
    b # trailing b comment
    # trailing b ownline comment
    +  # trailing second operator comment
    # leading c comment
    c # trailing c comment
    # trailing own line comment
 )


# Black breaks the right side first for the following expressions:
aaaaaaaaaaaaaa + caaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaal(argument1, argument2, argument3)
aaaaaaaaaaaaaa + [bbbbbbbbbbbbbbbbbbbbbb, ccccccccccccccccccccc, dddddddddddddddd, eeeeeee]
aaaaaaaaaaaaaa + (bbbbbbbbbbbbbbbbbbbbbb, ccccccccccccccccccccc, dddddddddddddddd, eeeeeee)
aaaaaaaaaaaaaa + { key1:bbbbbbbbbbbbbbbbbbbbbb, key2: ccccccccccccccccccccc, key3: dddddddddddddddd, key4: eeeeeee }
aaaaaaaaaaaaaa + { bbbbbbbbbbbbbbbbbbbbbb, ccccccccccccccccccccc, dddddddddddddddd, eeeeeee }
aaaaaaaaaaaaaa + [a for x in bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb ]
aaaaaaaaaaaaaa + (a for x in bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb )
aaaaaaaaaaaaaa + {a for x in bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb}

# Wraps it in parentheses if it needs to break both left and right
aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa + [
    bbbbbbbbbbbbbbbbbbbbbb,
    ccccccccccccccccccccc,
    dddddddddddddddd,
    eee
] # comment



# But only for expressions that have a statement parent.
not (aaaaaaaaaaaaaa + {a for x in bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb})
[a + [bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb] in c ]


# leading comment
(
    # comment
    content + b
)


if (
    aaaaaaaaaaaaaaaaaa +
    # has the child process finished?
    bbbbbbbbbbbbbbb +
    # the child process has finished, but the
    # transport hasn't been notified yet?
    ccccccccccc
):
    pass


# Left only breaks
if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & aaaaaaaaaaaaaaaaaaaaaaaaaa:
    pass

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:
    pass

# Right only can break
if aaaaaaaaaaaaaaaaaaaaaaaaaa & [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    pass

if aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa & [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    pass


# Left or right can break
if [2222, 333] & [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    pass

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & [2222, 333]:
    pass

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & [fffffffffffffffff, gggggggggggggggggggg, hhhhhhhhhhhhhhhhhhhhh, iiiiiiiiiiiiiiii, jjjjjjjjjjjjj]:
    pass

if (
    # comment
    [
        aaaaaaaaaaaaa,
        bbbbbbbbbbbbbbbbbbbb,
        cccccccccccccccccccc,
        dddddddddddddddddddd,
        eeeeeeeeee,
    ]
) & [
    fffffffffffffffff,
    gggggggggggggggggggg,
    hhhhhhhhhhhhhhhhhhhhh,
    iiiiiiiiiiiiiiii,
    jjjjjjjjjjjjj,
]:
    pass

    pass

# Nesting
if (aaaa + b) & [
    fffffffffffffffff,
    gggggggggggggggggggg,
    hhhhhhhhhhhhhhhhhhhhh,
    iiiiiiiiiiiiiiii,
    jjjjjjjjjjjjj,
]:
    pass

if [
    fffffffffffffffff,
    gggggggggggggggggggg,
    hhhhhhhhhhhhhhhhhhhhh,
    iiiiiiiiiiiiiiii,
    jjjjjjjjjjjjj,
] & (a + b):
    pass


if [
    fffffffffffffffff,
    gggggggggggggggggggg,
    hhhhhhhhhhhhhhhhhhhhh,
    iiiiiiiiiiiiiiii,
    jjjjjjjjjjjjj,
] & (
    # comment
    a
    + b
):
    pass

if (
    [
        fffffffffffffffff,
        gggggggggggggggggggg,
        hhhhhhhhhhhhhhhhhhhhh,
        iiiiiiiiiiiiiiii,
        jjjjjjjjjjjjj,
    ]
    &
    # comment
    a + b
):
    pass


# Unstable formatting in https://github.com/realtyem/synapse-unraid/blob/unraid_develop/synapse/handlers/presence.py
for user_id in set(target_user_ids) - {u.user_id for u in updates}:
    updates.append(UserPresenceState.default(user_id))

# If either operator is parenthesized, use non-simple formatting.
# See: https://github.com/astral-sh/ruff/issues/7318.
10**(2)
10**2

# Keeps parenthesized left hand sides
(
    log(self.price / self.strike)
    + (self.risk_free - self.div_cont + 0.5 * (self.sigma**2)) * self.exp_time
) / self.sigmaT

# Stability with end-of-line comments between empty tuples and bin op
x = () - (#
)
x = (
    ()
    - ()  #
)
x = (
    () - ()  #
)


# Avoid unnecessary parentheses around multiline strings.
expected_content = """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
""" % (
    self.base_url,
    date.today(),
)

expected_content = (
    """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
"""
    # Needs parentheses
    % (
    self.base_url,
    date.today(),
    )
)

expected_content = (
    """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
"""
    %
    # Needs parentheses
    (
        self.base_url,
        date.today(),
    )
)


expected_content = """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
""" + a.call.expression(
    self.base_url,
    date.today(),
)

expected_content = """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
""" + sssssssssssssssssssssssssssssssssssssssssooooo * looooooooooooooooooooooooooooooongggggggggggg

call(arg1, arg2, """
short
""", arg3=True)

expected_content = (
    """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
"""
    %
    (
        self.base_url
    )
)


expected_content = (
    """<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
<sitemap><loc>%s/simple/sitemap-simple.xml</loc><lastmod>%s</lastmod>
</sitemap>
</sitemapindex>
"""
    %
    (
        # Needs parentheses
        self.base_url
    )
)

# Skip FString content when determining whether to omit optional parentheses or not.0
# The below expression should be parenthesized because it ends with an fstring and starts with a name.
# (Call expressions at the beginning don't count as parenthesized because they don't start with parens).
assert (
    format.format_event(spec)
    == f'Event("_remove_cookie", {{key:`testkey`,options:{json.dumps(options)}}})'
)
# Avoid parentheses for this example because it starts with a tuple expression.
assert (
    (spec, format)
    == f'Event("_remove_cookie", {{key:`testkey`,options:{json.dumps(options)}}})'
)

rowuses = [(1 << j) |                  # column ordinal
           (1 << (n + i-j + n-1)) |    # NW-SE ordinal
           (1 << (n + 2*n-1 + i+j))    # NE-SW ordinal
           for j in rangen]

rowuses = [((1 << j) # column ordinal
         )|
           (
               # comment
               (1 << (n + i-j + n-1))) |    # NW-SE ordinal
           (1 << (n + 2*n-1 + i+j))    # NE-SW ordinal
           for j in rangen]

skip_bytes = (
    header.timecnt * 5  # Transition times and types
    + header.typecnt * 6  # Local time type records
    + header.charcnt  # Time zone designations
    + header.leapcnt * 8  # Leap second records
    + header.isstdcnt  # Standard/wall indicators
    + header.isutcnt  # UT/local indicators
)


if (
    (1 + 2)  # test
    or (3 + 4)  # other
    or (4 + 5)  # more
):
    pass


if (
    (1 and 2)  # test
    + (3 and 4)  # other
    + (4 and 5)  # more
):
    pass


if (
    (1 + 2)  # test
    < (3 + 4)  # other
    > (4 + 5)  # more
):
    pass

z = (
                 a
                 +
                 # a: extracts this comment
                 (
                     # b: and this comment
                     (
                         # c: formats it as part of the expression
                         x and y
                     )
             )
)

z = (
    (

        (

            x and y
            # a: formats it as part of the expression

        )
        # b: extracts this comment

    )
    # c: and this comment
    + a
)

# Test for https://github.com/astral-sh/ruff/issues/7431
if True:
    if True:
        if True:
            if True:
                msg += " " + _(
                    "Since the role is not mentionable, it will be momentarily made mentionable "
                    "when announcing a streamalert. Please make sure I have the correct "
                    "permissions to manage this role, or else members of this role won't receive "
                    "a notification."
                )
