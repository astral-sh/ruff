# Expressions with empty parentheses.
ct_match = (
    unicodedata.normalize("NFKC", s1).casefold()
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(1)
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(0)
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(1)
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(1)
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(1)
)

ct_match = (
    unicodedata.normalize("NFKC", s1).casefold(
        # foo
    )
    == unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold(
        # foo
    )
)

ct_match = (
    [].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [].unicodedata.normalize("NFKC", s1).casefold()
    == [1].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [1].unicodedata.normalize("NFKC", s1).casefold()
    == [].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    [1].unicodedata.normalize("NFKC", s1).casefold()
    == [1].unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    {}.unicodedata.normalize("NFKC", s1).casefold()
    == {}.unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    {}.unicodedata.normalize("NFKC", s1).casefold()
    == {1}.unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    {1}.unicodedata.normalize("NFKC", s1).casefold()
    == {}.unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    {1}.unicodedata.normalize("NFKC", s1).casefold()
    == {1}.unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

ct_match = (
    ([]).unicodedata.normalize("NFKC", s1).casefold()
    == ([]).unicodedata.normalize("NFKCNFKCNFKCNFKCNFKC", s2).casefold()
)

return await self.http_client.fetch(
    f"http://127.0.0.1:{self.port}{path}", method=method, **kwargs,
)

return await self.http_client().fetch(
    f"http://127.0.0.1:{self.port}{path}", method=method, **kwargs,
)

return await self().http_client().fetch(
    f"http://127.0.0.1:{self.port}{path}", method=method, **kwargs,
)

response = await sync_to_async(
    lambda: self.django_handler.get_response(request), thread_sensitive=True
)()
