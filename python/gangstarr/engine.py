from __future__ import annotations

import json
from typing import Any

from gangstarr.gangstarr import analyze_events as _analyze_events
from gangstarr.gangstarr import fingerprint_sql, normalize_sql
from gangstarr.schemas import QueryEvent

__all__ = ["normalize_sql", "fingerprint_sql", "analyze"]


def analyze(events: list[QueryEvent]) -> dict[str, Any]:
    """Run Rust analysis engine on a list of QueryEvent objects.

    Returns a dict with keys: summary, groups, findings.
    """
    events_json = json.dumps([e.to_dict() for e in events])
    return _analyze_events(events_json)
