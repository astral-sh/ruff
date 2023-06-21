(aaaaaaaa
    + # trailing operator comment
    b # trailing right comment
)


(aaaaaaaa # trailing left comment
    +  # trailing operator comment
    # leading right comment
    b
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
    ...

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:
    ...

# Right only can break
if aaaaaaaaaaaaaaaaaaaaaaaaaa & [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    ...

if aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa & [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    ...


# Left or right can break
if [2222, 333] & [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
]:
    ...

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & [2222, 333]:
    ...

if [
    aaaaaaaaaaaaa,
    bbbbbbbbbbbbbbbbbbbb,
    cccccccccccccccccccc,
    dddddddddddddddddddd,
    eeeeeeeeee,
] & [fffffffffffffffff, gggggggggggggggggggg, hhhhhhhhhhhhhhhhhhhhh, iiiiiiiiiiiiiiii, jjjjjjjjjjjjj]:
    ...

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

    ...

# Nesting
if (aaaa + b) & [
    fffffffffffffffff,
    gggggggggggggggggggg,
    hhhhhhhhhhhhhhhhhhhhh,
    iiiiiiiiiiiiiiii,
    jjjjjjjjjjjjj,
]:
    ...

if [
    fffffffffffffffff,
    gggggggggggggggggggg,
    hhhhhhhhhhhhhhhhhhhhh,
    iiiiiiiiiiiiiiii,
    jjjjjjjjjjjjj,
] & (a + b):
    ...


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
    ...

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
    ...
