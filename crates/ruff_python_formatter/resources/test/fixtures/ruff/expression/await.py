# Regression test for: https://github.com/astral-sh/ruff/issues/7420
result = await self.request(
    f"/applications/{int(application_id)}/guilds/{int(scope)}/commands/{int(command_id)}/permissions"
)

result = await (self.request(
    f"/applications/{int(application_id)}/guilds/{int(scope)}/commands/{int(command_id)}/permissions"
))

result = await (1 + f(1, 2, 3,))

result = (await (1 + f(1, 2, 3,)))
