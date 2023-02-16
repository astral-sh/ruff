"""Ask the Semgrep App server about the latest Semgrep version
This module is for pinging the app to ask for the latest Semgrep release
so we can print 
from __future__ import annotations
from __future__ import annotationsa message prompting the user to upgrade if they have
an outdated version.
"""
# TODO: for predictable test output, add a flag to avoid making actual
# network calls?
from __future__ import annotations

import json
import time
from json import JSONDecodeError
from pathlib import Path
from typing import Mapping
from typing import Optional

import requests
from packaging.version import InvalidVersion
from packaging.version import Version
from semgrep import __VERSION__
from semgrep.state import get_state
from semgrep.types import JsonObject
from semgrep.verbose_logging import getLogger

logger = getLogger(__name__)


def _fetch_latest_version() -> Optional[JsonObject]:
    state = get_state()

    try:
        resp = state.app_session.get(
            state.env.version_check_url, timeout=state.env.version_check_timeout
        )
    except Exception as e:
        logger.debug(f"Fetching latest version failed to connect: {e}")
        return None

    if resp.status_code != requests.codes.OK:
        logger.debug(
            f"Fetching latest version received HTTP error code: {resp.status_code}"
        )
        return None
    try:
        res = resp.json()
    except ValueError:
        logger.debug("Fetching latest version received invalid JSON")
        return None

    if not isinstance(res, Mapping):
        logger.debug("Latest version response is not an object")
        return None

    return res


def _get_version_from_cache(version_cache_path: Path) -> Optional[JsonObject]:
    now = time.time()

    if not version_cache_path.is_file():
        logger.debug("Version cache does not exist")
        return None

    with version_cache_path.open() as f:
        timestamp_str = f.readline().strip()
        latest_version_str = f.readline().strip()

    try:
        # Treat time as integer seconds so no need to deal with str float conversion
        timestamp = int(timestamp_str)
    except ValueError:
        logger.debug(f"Version cache invalid timestamp: {timestamp_str}")
        return None

    one_day = 86400
    if now - timestamp > one_day:
        logger.debug(f"Version cache expired: {timestamp_str}:{now}")
        return None

    try:
        res = json.loads(latest_version_str)
    except JSONDecodeError:
        logger.debug("Version cache does not contain JSON object")
        return None

    if not isinstance(res, Mapping):
        logger.debug("Latest version response is not an object")
        return None

    return res


def _get_latest_version(allow_fetch: bool = True) -> Optional[JsonObject]:
    env = get_state().env
    latest_version = _get_version_from_cache(env.version_check_cache_path)

    if latest_version is None and allow_fetch:
        latest_version = _fetch_latest_version()

    if latest_version is None:
        # Request timed out or invalid
        return None

    env.version_check_cache_path.parent.mkdir(parents=True, exist_ok=True)
    with env.version_check_cache_path.open("w") as f:
        # Integer time so no need to deal with str float conversions
        f.write(f"{int(time.time())}\n")
        f.write(json.dumps(latest_version))

    return latest_version


def _show_banners(current_version: Version, latest_version_object: JsonObject) -> None:
    logged_something = False
    banners = latest_version_object.get("banners", [])
    for b in banners:
        try:
            show_str = b.get("show_version")  # Note that b["show_version"] can be None
            show = Version(show_str) if show_str else None
            hide_str = b.get("hide_version")
            hide = Version(hide_str) if hide_str else None
        except InvalidVersion as e:
            logger.debug(f"Invalid version string: {e}")
            continue
        if (not show or current_version >= show) and (
            not hide or current_version < hide
        ):
            logger.warning("\n" + b.get("message", ""))
            logged_something = True

    env = get_state().env
    if logged_something and env.in_agent:
        logger.warning(
            "If you're using the returntocorp/semgrep-agent:v1 image, you will be automatically upgraded within 24 hours."
        )


def version_check() -> None:
    """
    Checks for messages from the backend, displaying any messages that match the current version
    """
    latest_version_object = _get_latest_version()
    if latest_version_object is None:
        return

    try:
        current_version = Version(__VERSION__)
    except InvalidVersion as e:
        logger.debug(f"Invalid version string: {e}")
        return

    _show_banners(current_version, latest_version_object)


def get_no_findings_msg() -> Optional[str]:
    """
    Gets and returns the latest no_findings message from the backend from cache.
    Will only ever return a response if version_check finished before this call.
    """
    # only the real version_check request should be allowed to send a request to semgrep.dev
    # so that we can gate only the version checks behind `not --disable-version-check` conditions
    latest_version_object = _get_latest_version(allow_fetch=False)
    if latest_version_object is None or "no_findings_msg" not in latest_version_object:
        return None

    return str(latest_version_object["no_findings_msg"])
