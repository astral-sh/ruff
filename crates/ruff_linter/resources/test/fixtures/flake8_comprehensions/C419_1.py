sum([x.val for x in bar])
min([x.val for x in bar])
max([x.val for x in bar])
sum([x.val for x in bar], 0)

# OK
sum(x.val for x in bar)
min(x.val for x in bar)
max(x.val for x in bar)
sum((x.val for x in bar), 0)

# Multi-line
sum(
    [
        delta
        for delta in timedelta_list
        if delta
    ],
    dt.timedelta(),
)
