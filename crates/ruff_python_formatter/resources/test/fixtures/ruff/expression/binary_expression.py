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
