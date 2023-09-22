# Regression test for: https://github.com/astral-sh/ruff/issues/7420
result = await self.request(
    f"/applications/{int(application_id)}/guilds/{int(scope)}/commands/{int(command_id)}/permissions"
)

result = await (self.request(
    f"/applications/{int(application_id)}/guilds/{int(scope)}/commands/{int(command_id)}/permissions"
))

result = await (1 + f(1, 2, 3,))

result = (await (1 + f(1, 2, 3,)))

# Optional parentheses.
await foo
await (foo)
await foo()
await (foo())
await []()
await ([]())
await (foo + bar)()
await ((foo + bar)())
await foo.bar
await (foo.bar)
await foo['bar']
await (foo['bar'])
await 1
await (1)
await ""
await ("")
await f""
await (f"")
await [foo]
await ([foo])
await {foo}
await ({foo})
await (lambda foo: foo)
await (foo or bar)
await (foo * bar)
await (yield foo)
await (not foo)
await 1, 2, 3
await (1, 2, 3)
await ( # comment
    [foo]
)
await (
    # comment
    [foo]
)
