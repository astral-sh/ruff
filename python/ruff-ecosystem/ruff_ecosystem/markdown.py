from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ruff_ecosystem.projects import CommandOptions, Project


def markdown_sections(
    summary: str, sections: list[str], max_length: int | None = None
) -> str:
    contents = "\n\n".join([summary, *sections])
    if max_length is None or len(contents) <= max_length:
        return contents

    notice = "_Some detail sections are omitted from this summary._"
    remaining = max_length - len(summary) - len(notice) - 2
    if remaining < 0:
        raise ValueError("The Markdown limit is too small for the summary.")
    included = [summary]
    for section in sections:
        if len(section) + 2 <= remaining:
            included.append(section)
            remaining -= len(section) + 2
    return "\n\n".join([*included, notice])


def markdown_project_section(
    title: str, content: str | list[str], options: CommandOptions, project: Project
) -> list[str]:
    return markdown_details(
        summary=f'<a href="{project.repo.url}">{project.repo.fullname}</a> ({title})',
        content=content,
        preface=(
            # Show the command used for the check if the options are non-default
            "<pre>ruff " + " ".join(options.to_ruff_args()) + "</pre>"
            if options != type(options)()
            else None
        ),
    )


def markdown_plus_minus(added: int, removed: int) -> str:
    # TODO(zanieb): GitHub does not support coloring with <span> it seems like the only
    #               way is to use LateX `${\text{\color{green}+10 \color{red}-10}}$` but
    #               it renders so ugly it's not worth doing yet
    return f"+{added} -{removed}"


def markdown_details(
    summary: str, content: str | list[str], preface: str | None
) -> list[str]:
    lines: list[str] = []
    lines.append(f"<details><summary>{summary}</summary>")
    if preface:
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
