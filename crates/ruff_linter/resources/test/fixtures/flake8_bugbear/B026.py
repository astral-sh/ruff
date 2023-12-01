"""
Should emit:
B026 - on lines 16, 17, 18, 19, 20, 21
"""


def foo(bar, baz, bam):
    print(bar, baz, bam)


bar_baz = ["bar", "baz"]

foo("bar", "baz", bam="bam")
foo("bar", baz="baz", bam="bam")
foo(bar="bar", baz="baz", bam="bam")
foo(bam="bam", *["bar", "baz"])
foo(bam="bam", *bar_baz)
foo(baz="baz", bam="bam", *["bar"])
foo(bar="bar", baz="baz", bam="bam", *[])
foo(bam="bam", *["bar"], *["baz"])
foo(*["bar"], bam="bam", *["baz"])
