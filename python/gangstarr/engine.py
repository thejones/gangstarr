from __future__ import annotations

import datetime
import json
import time
from pathlib import Path
from typing import Any

from gangstarr.gangstarr import analyze_events as _analyze_events
from gangstarr.gangstarr import (
    correlate_run as _correlate_run,
    fingerprint_sql,
    get_run_history as _get_run_history,
    init_gangstarr_db as _init_gangstarr_db,
    normalize_sql,
    store_field_usage as _store_field_usage,
    store_runtime_run as _store_runtime_run,
    store_static_run as _store_static_run,
)
from gangstarr.schemas import QueryEvent

__all__ = [
    "normalize_sql",
    "fingerprint_sql",
    "analyze",
    "store_runtime_findings",
    "store_static_findings",
    "store_field_usage_records",
    "get_history",
    "correlate",
]


# ── Internal helpers ──────────────────────────────────────────────────────────


def _make_run_id() -> str:
    """Generate a hex run ID from the current time in milliseconds."""
    return f"{int(time.time() * 1000):016x}"


def _get_db_path(project_root: str) -> str:
    """Return the canonical DB path for a given project root."""
    return str(Path(project_root) / ".gangstarr" / "gangstarr.db")


def _now_iso() -> str:
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


# ── Core analysis ─────────────────────────────────────────────────────────────


def analyze(events: list[QueryEvent]) -> dict[str, Any]:
    """Run Rust analysis engine on a list of QueryEvent objects.

    Returns a dict with keys: summary, groups, findings.
    """
    events_json = json.dumps([e.to_dict() for e in events])
    return _analyze_events(events_json)


# ── Storage ───────────────────────────────────────────────────────────────────


def store_runtime_findings(
    events: list[QueryEvent],
    analysis: dict[str, Any],
    project_root: str,
    run_id: str | None = None,
) -> str:
    """Persist a runtime-analysis run to the gangstarr SQLite database.

    ``analysis`` is the dict returned by :func:`analyze`.  Returns the
    ``run_id`` used, which can be passed to :func:`correlate` afterwards.
    """
    if run_id is None:
        run_id = _make_run_id()
    db_path = _get_db_path(project_root)
    _init_gangstarr_db(db_path)
    _store_runtime_run(db_path, run_id, _now_iso(), project_root, json.dumps(analysis))
    return run_id


def store_static_findings(
    findings: list[dict[str, Any]],
    project_root: str,
    run_id: str | None = None,
) -> str:
    """Persist a static-analysis run to the gangstarr SQLite database.

    ``findings`` is the list of finding dicts (as returned by the CLI JSON
    output or converted from ``StaticFinding`` structs).  Returns the
    ``run_id`` used.
    """
    if run_id is None:
        run_id = _make_run_id()
    db_path = _get_db_path(project_root)
    _init_gangstarr_db(db_path)
    _store_static_run(db_path, run_id, _now_iso(), project_root, json.dumps(findings))
    return run_id


def store_field_usage_records(
    records: list[dict[str, str]],
    project_root: str,
    run_id: str,
) -> None:
    """Persist field-usage records (from ``flush_field_usage()``) for a run.

    ``records`` is the list returned by
    :func:`gangstarr.field_tracker.flush_field_usage`.
    """
    db_path = _get_db_path(project_root)
    _init_gangstarr_db(db_path)
    _store_field_usage(db_path, run_id, json.dumps(records))


# ── History & correlation ─────────────────────────────────────────────────────


def get_history(project_root: str, limit: int = 20) -> list[dict[str, Any]]:
    """Return the most recent ``limit`` analysis runs from the local DB.

    Each entry has keys: ``run_id``, ``created_at``, ``run_type``,
    ``project_root``, ``static_count``, ``runtime_count``.
    """
    db_path = _get_db_path(project_root)
    return json.loads(_get_run_history(db_path, limit))  # type: ignore[arg-type]


def correlate(run_id: str, project_root: str) -> list[dict[str, Any]]:
    """Cross-reference a static run against runtime evidence.

    Returns a list of correlation dicts.  Each has at minimum:
    ``kind``, ``static_rule``, ``file``, ``line``, ``message``,
    ``escalated``, ``original_severity``, ``escalated_severity``.
    """
    db_path = _get_db_path(project_root)
    return json.loads(_correlate_run(db_path, run_id))  # type: ignore[arg-type]
