from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ruff_ecosystem.projects import Project


def markdown_project_section(
    title: str, content: str | list[str], options: object, project: Project
) -> list[str]:
    return markdown_details(
        summary=f'<a href="{project.repo.url}">{project.repo.fullname}</a> ({title})',
        # Show the command used for the check
        preface="<pre>ruff " + " ".join(options.to_cli_args()) + "</pre>",
        content=content,
    )


def markdown_plus_minus(added: int, removed: int) -> str:
    # TODO(zanieb): GitHub does not support coloring with <span> it seems like the only
    #               way is to use LateX `${\text{\color{green}+10 \color{red}-10}}$` but
    #               it renders so ugly it's not worth doing yet
    return f"+{added} -{removed}"


def markdown_details(summary: str, preface: str, content: str | list[str]):
    lines = []
    lines.append(f"<details><summary>{summary}</summary>")
    lines.append("<p>")
    lines.append(preface)
    lines.append("</p>")
    lines.append("<p>")
    lines.append("")

    if isinstance(content, str):
        lines.append(content)
    else:
        lines.extend(content)

    lines.append("")
    lines.append("</p>")
    lines.append("</details>")
    return lines
