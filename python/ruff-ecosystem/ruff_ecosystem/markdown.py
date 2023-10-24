from __future__ import annotations

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ruff_ecosystem.projects import Project


def project_section(
    title: str, content: str, options: str, project: Project
) -> list[str]:
    lines = []
    lines.append(
        f'<details><summary><a href="{project.repo.url}">{project.repo.fullname}</a> ({title})</summary>'
    )
    lines.append(options)
    lines.append("<p>")
    lines.append("")

    lines.append(content)

    lines.append("")
    lines.append("</p>")
    lines.append("</details>")
    return lines
