# Test cases for call chains and optional parentheses, with and without fluent style

raise OsError("") from a.aaaaa(
    aksjdhflsakhdflkjsadlfajkslhfdkjsaldajlahflashdfljahlfksajlhfajfjfsaahflakjslhdfkjalhdskjfa
).a(aaaa)

raise OsError(
    "sökdjffffsldkfjlhsakfjhalsökafhsöfdahsödfjösaaksjdllllllllllllll"
) from a.aaaaa(
    aksjdhflsakhdflkjsadlfajkslhfdkjsaldajlahflashdfljahlfksajlhfajfjfsaahflakjslhdfkjalhdskjfa
).a(
    aaaa
)

a1 = Blog.objects.filter(entry__headline__contains="Lennon").filter(
    entry__pub_date__year=2008
)

a2 = Blog.objects.filter(
    entry__headline__contains="Lennon",
).filter(
    entry__pub_date__year=2008,
)

raise OsError("") from (
    Blog.objects.filter(
        entry__headline__contains="Lennon",
    )
    .filter(
        entry__pub_date__year=2008,
    )
    .filter(
        entry__pub_date__year=2008,
    )
)

raise OsError("sökdjffffsldkfjlhsakfjhalsökafhsöfdahsödfjösaaksjdllllllllllllll") from (
    Blog.objects.filter(
        entry__headline__contains="Lennon",
    )
    .filter(
        entry__pub_date__year=2008,
    )
    .filter(
        entry__pub_date__year=2008,
    )
)

# Break only after calls and indexing
b1 = (
    session.query(models.Customer.id)
    .filter(
        models.Customer.account_id == account_id, models.Customer.email == email_address
    )
    .count()
)

b2 = (
    Blog.objects.filter(
        entry__headline__contains="Lennon",
    )
    .limit_results[:10]
    .filter(
        entry__pub_date__month=10,
    )
)

# Nested call chains
c1 = (
    Blog.objects.filter(
        entry__headline__contains="Lennon",
    ).filter(
        entry__pub_date__year=2008,
    )
    + Blog.objects.filter(
        entry__headline__contains="McCartney",
    )
    .limit_results[:10]
    .filter(
        entry__pub_date__year=2010,
    )
).all()

# Test different cases with trailing end of line comments:
# * fluent style, fits: no parentheses -> ignore the expand_parent
# * fluent style, doesn't fit: break all soft line breaks
# * default, fits: no parentheses
# * default, doesn't fit: parentheses but no soft line breaks

# Fits, either style
d11 = x.e().e().e() #
d12 = (x.e().e().e()) #
d13 = (
    x.e() #
    .e()
    .e()
)

# Doesn't fit, default
d2 = (
    x.e().esadjkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkfsdddd()  #
)

# Doesn't fit, fluent style
d3 = (
    x.e()  #
    .esadjkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk()
    .esadjkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk()
)

# Don't drop the bin op parentheses
e1 = (1 + 2).w().t()
e2 = (1 + 2)().w().t()
e3 = (1 + 2)[1].w().t()

# Treat preserved parentheses correctly
f1 = (b().c()).d(1,)
f2 = b().c().d(1,)
f3 = (b).c().d(1,)
f4 = (a)(b).c(1,)
f5 = (a.b()).c(1,)

# Indent in the parentheses without breaking
g1 = (
    queryset.distinct().order_by(field.name).values_list(field_name_flat_long_long=True)
)

# Fluent style in subexpressions
if (
    not a()
    .b()
    .cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc()
):
    pass
h2 = (
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
    + ccccccccccccccccccccccccc()
    .dddddddddddddddddddddd()
    .eeeeeeeeee()
    .ffffffffffffffffffffff()
)

# Parentheses aren't allowed on statement level, don't use fluent style here
if True:
    (alias).filter(content_typeold_content_type).update(
        content_typenew_contesadfasfdant_type
    )

zero(
    one,
).two(
    three,
).four(
    five,
)

max_message_id = (
    Message.objects.filter(recipient=recipient).order_by("id").reverse()[0].id
)

max_message_id = (
    Message.objects.filter(recipient=recipient).order_by("id").reverse()[0].id()
)

# Parentheses with fluent style within and outside of the parentheses.
(
    (
        df1_aaaaaaaaaaaa.merge()
    )
    .groupby(1,)
    .sum()
)

(
    ( # foo
        df1_aaaaaaaaaaaa.merge()
    )
    .groupby(1,)
    .sum()
)

(
    (
        df1_aaaaaaaaaaaa.merge()
        .groupby(1,)
    )
    .sum()
)


(
    (
        df1_aaaaaaaaaaaa.merge()
        .groupby(1,)
    )
    .sum()
    .bar()
)

(
    (
        df1_aaaaaaaaaaaa.merge()
        .groupby(1,)
        .bar()
    )
    .sum()
)

(
    (
        df1_aaaaaaaaaaaa.merge()
        .groupby(1,)
        .bar()
    )
    .sum()
    .baz()
)

# Note in preview we split at `pl` which some
# folks may dislike. (Similarly with common
# `np` and `pd` invocations).
#
# This is because we cannot reliably predict,
# just from syntax, whether a short identifier
# is being used as a 'namespace' or as an 'object'.
#
# As of 2025.12.15, we do not indent methods in
# fluent formatting. If we ever decide to do so,
# it may make sense to special case call chain roots
# that are shorter than the indent-width (like Prettier does).
# This would have the benefit of handling these common
# two-letter aliases for libraries.


expr = (
    pl.scan_parquet("/data/pypi-parquet/*.parquet")
    .filter(
        [
            pl.col("path").str.contains(
                r"\.(asm|c|cc|cpp|cxx|h|hpp|rs|[Ff][0-9]{0,2}(?:or)?|go)$"
            ),
            ~pl.col("path").str.contains(r"(^|/)test(|s|ing)"),
            ~pl.col("path").str.contains("/site-packages/", literal=True),
        ]
    )
    .with_columns(
        month=pl.col("uploaded_on").dt.truncate("1mo"),
        ext=pl.col("path")
        .str.extract(pattern=r"\.([a-z0-9]+)$", group_index=1)
        .str.replace_all(pattern=r"cxx|cpp|cc|c|hpp|h", value="C/C++")
        .str.replace_all(pattern="^f.*$", value="Fortran")
        .str.replace("rs", "Rust", literal=True)
        .str.replace("go", "Go", literal=True)
        .str.replace("asm", "Assembly", literal=True)
        .replace({"": None}),
    )
    .group_by(["month", "ext"])
    .agg(project_count=pl.col("project_name").n_unique())
    .drop_nulls(["ext"])
    .sort(["month", "project_count"], descending=True)
)

def indentation_matching_for_loop_in_preview():
    if make_this:
        if more_nested_because_line_length:
            identical_hidden_layer_sizes = all(
            current_hidden_layer_sizes == first_hidden_layer_sizes
            for current_hidden_layer_sizes in self.component_config[
                HIDDEN_LAYERS_SIZES
            ].values().attr
           )

def indentation_matching_walrus_in_preview():
    if make_this:
        if more_nested_because_line_length:
            with self.read_ctx(book_type) as cursor:
                if (entry_count := len(names := cursor.execute(
                    'SELECT name FROM address_book WHERE address=?',
                    (address,),
                ).fetchall().some_attr)) == 0 or len(set(names)) > 1:
                    return

# behavior with parenthesized roots
x = (aaaaaaaaaaaaaaaaaaaaaa).bbbbbbbbbbbbbbbbbbb.cccccccccccccccccccccccc().dddddddddddddddddddddddd().eeeeeeeeeeee
