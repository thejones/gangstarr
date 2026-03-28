from __future__ import annotations

import uuid
from dataclasses import dataclass, field
from typing import Any


@dataclass
class QueryEvent:
    """A single captured SQL query execution."""

    sql: str
    duration_ms: float
    file: str
    line: int
    function: str
    source: str
    label: str | None = None
    request_id: str | None = None
    db_alias: str = "default"
    resolver_path: str = ""
    caller_chain: list[dict[str, Any]] = field(default_factory=list)

    def to_dict(self) -> dict[str, Any]:
        return {
            "sql": self.sql,
            "duration_ms": self.duration_ms,
            "file": self.file,
            "line": self.line,
            "function": self.function,
            "source": self.source,
            "label": self.label,
            "request_id": self.request_id,
            "db_alias": self.db_alias,
            "resolver_path": self.resolver_path,
            "caller_chain": self.caller_chain,
        }


@dataclass
class RequestContext:
    """Metadata about the HTTP request being profiled."""

    method: str = ""
    path: str = ""
    view_name: str = ""
    status_code: int | None = None
    request_id: str = field(default_factory=lambda: uuid.uuid4().hex[:12])
    operation_name: str = ""
    operation_type: str = ""
    # Discipline: cross-request tracing fingerprints
    client_fingerprint: str = ""
    client_fp_source: str = ""
    request_fingerprint: str = ""
