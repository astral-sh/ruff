from enum import Enum
import abc
from ruff_ecosystem.models import Target, Diff, ClonedRepository, Result
from ruff_ecosystem.ruff import CHECK_DIFF_LINE_RE
import traceback
import json
from pathlib import Path
import dataclasses


class Emitter(abc.ABC):
    @abc.abstractclassmethod
    def emit_error(cls, target: Target, exc: Exception):
        pass

    @abc.abstractclassmethod
    def emit_diff(cls, target: Target, diff: Diff, cloned_repo: ClonedRepository):
        pass

    @abc.abstractclassmethod
    def emit_result(cls, result: Result):
        pass


class DebugEmitter(Emitter):
    def emit_error(cls, target: Target, exc: Exception):
        print(f"Error in {target.repo.fullname}")
        traceback.print_exception(exc)

    def emit_diff(cls, target: Target, diff: Diff, cloned_repo: ClonedRepository):
        pass


class JSONEmitter(Emitter):
    class DataclassJSONEncoder(json.JSONEncoder):
        def default(self, o):
            if dataclasses.is_dataclass(o):
                return dataclasses.asdict(o)
            if isinstance(o, set):
                return tuple(o)
            if isinstance(o, Path):
                return str(o)
            return super().default(o)

    def emit_error(cls, target: Target, exc: Exception):
        pass

    def emit_diff(cls, target: Target, diff: Diff, cloned_repo: ClonedRepository):
        pass

    def emit_result(cls, result: Result):
        print(json.dumps(result, indent=4, cls=cls.DataclassJSONEncoder))


class MarkdownEmitter(Emitter):
    def emit_error(cls, target: Target, exc: Exception):
        cls._print(title="error", content=f"```\n{exc}\n```", target=target)

    def emit_diff(cls, target: Target, diff: Diff, cloned_repo: ClonedRepository):
        changes = f"+{len(diff.added)}, -{len(diff.removed)}"

        content = ""
        for line in list(diff):
            match = CHECK_DIFF_LINE_RE.match(line)
            if match is None:
                content += line + "\n"
                continue

            pre, inner, path, lnum, post = match.groups()
            url = cloned_repo.url_for(path, int(lnum))
            content += f"{pre} <a href='{url}'>{inner}</a> {post}" + "\n"

        cls._print(title=changes, content=f"<pre>\n{content}\n</pre>", target=target)

    def _print(cls, title: str, content: str, target: Target):
        print(f"<details><summary>{target.repo.fullname} ({title})</summary>")
        print(target.repo.url, target.check_options.summary())
        print("<p>")
        print()

        print(content)

        print()
        print("</p>")
        print("</details>")


class EmitterType(Enum):
    markdown = "markdown"
    json = "json"

    def to_emitter(self) -> Emitter:
        match self:
            case self.markdown:
                return MarkdownEmitter()
            case self.json:
                return JSONEmitter()
            case _:
                raise ValueError("Unknown emitter type {self}")
