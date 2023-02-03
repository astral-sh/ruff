import re

PYLINT_DESCRIPTION_REGEX = re.compile(
    r"\|\n\| PL(?P<msgid>[CEWR][0-9]{4}) \| (?P<symbol>[a-z\-]+) \|",
)
PREFIX_TO_URL = {
    "F": "fatal",  # should never be used
    "E": "error",
    "W": "warning",
    "C": "convention",
    "R": "refactor",
}
PYLINT_BASE_URL = "https://pylint.readthedocs.io/en/latest/user_guide/messages/"


def get_link(msgid: str, symbol: str) -> str:
    return f"[{symbol}]({PYLINT_BASE_URL}{PREFIX_TO_URL[msgid[0]]}/{symbol}.html)"


if __name__ == "__main__":
    readme_path = "README.md"
    with open(readme_path, encoding="utf8") as f:
        content = f.read()
    for msgid, symbol in PYLINT_DESCRIPTION_REGEX.findall(content):
        content = content.replace(symbol, get_link(msgid, symbol))
    with open(readme_path, "w", encoding="utf8") as f:
        f.write(content)
