"""Discipline — cross-request duplicate tracing for gangstarr.

Named after the Gang Starr track "Discipline" — enforcing request
discipline by detecting wasteful cross-request patterns from SPA clients.

The module provides:
- Client fingerprinting via a priority waterfall (OTEL → correlation headers → IP+UA)
- Request fingerprinting (method + path + operation + variables hash)
- An in-memory, thread-safe ring buffer that detects duplicate requests within a time window
"""

from __future__ import annotations

import hashlib
import logging
import threading
from collections import deque
from dataclasses import dataclass, field
from typing import Any

from django.conf import settings

logger = logging.getLogger('gangstarr.discipline')

# ── Client fingerprint waterfall ──────────────────────────────────────────────

# Priority-ordered list of (django_meta_key, source_label).
# Django normalises HTTP headers to HTTP_<UPPER_UNDERSCORE> in request.META.
_CORRELATION_HEADERS: list[tuple[str, str]] = [
    # 1. W3C Trace Context (OpenTelemetry)
    ('HTTP_TRACEPARENT', 'otel'),
    # 2. Common correlation / distributed-tracing headers
    ('HTTP_X_CORRELATION_ID', 'x-correlation-id'),
    ('HTTP_X_REQUEST_ID', 'x-request-id'),
    ('HTTP_X_AMZN_TRACE_ID', 'x-amzn-trace-id'),
    ('HTTP_X_CLOUD_TRACE_CONTEXT', 'x-cloud-trace-context'),
    ('HTTP_X_B3_TRACEID', 'x-b3-traceid'),
]


def _extract_otel_trace_id(traceparent: str) -> str:
    """Extract the trace-id from a W3C traceparent header.

    Format: ``{version}-{trace-id}-{parent-id}-{trace-flags}``
    e.g. ``00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01``
    """
    parts = traceparent.strip().split('-')
    if len(parts) >= 2:
        return parts[1]
    return traceparent


def resolve_client_fingerprint(request) -> tuple[str, str]:
    """Resolve a client fingerprint from the request using a priority waterfall.

    Returns ``(fingerprint, source)`` where *source* indicates which header
    or method was used (e.g. ``"otel"``, ``"x-request-id"``, ``"ip+ua"``).
    """
    meta = request.META

    # Walk the waterfall — first match wins.
    for meta_key, source in _CORRELATION_HEADERS:
        value = meta.get(meta_key, '')
        if value:
            if source == 'otel':
                value = _extract_otel_trace_id(value)
            return value, source

    # Fallback: hash(IP + User-Agent), optionally mixed with auth token.
    ip = (
        meta.get('HTTP_X_FORWARDED_FOR', '').split(',')[0].strip()
        or meta.get('REMOTE_ADDR', '')
    )
    ua = meta.get('HTTP_USER_AGENT', '')
    raw = f'{ip}:{ua}'

    auth = meta.get('HTTP_AUTHORIZATION', '')
    if auth:
        raw += f':{hashlib.sha256(auth.encode()).hexdigest()[:16]}'

    fp = hashlib.sha256(raw.encode()).hexdigest()[:24]
    return fp, 'ip+ua'


# ── Request fingerprinting ────────────────────────────────────────────────────


def compute_request_fingerprint(
    method: str,
    path: str,
    operation_name: str = '',
    variables_hash: str = '',
) -> str:
    """Compute a deterministic fingerprint for a request.

    Two requests with the same fingerprint asked for the same thing.
    """
    raw = f'{method}:{path}:{operation_name}:{variables_hash}'
    return hashlib.sha256(raw.encode()).hexdigest()[:24]


def hash_graphql_variables(body_data: dict[str, Any] | None) -> str:
    """SHA-256 hash of the serialised GraphQL variables, or empty string."""
    if not body_data:
        return ''
    variables = body_data.get('variables')
    if not variables:
        return ''
    import json

    try:
        serialised = json.dumps(variables, sort_keys=True, default=str)
    except (TypeError, ValueError):
        return ''
    return hashlib.sha256(serialised.encode()).hexdigest()[:16]


# ── Data classes ──────────────────────────────────────────────────────────────


@dataclass
class RequestTrace:
    """A single profiled request in the ring buffer."""

    timestamp: float
    client_fingerprint: str
    client_fp_source: str
    request_fingerprint: str
    request_id: str
    method: str
    path: str
    operation_name: str = ''
    operation_type: str = ''
    sql_fingerprints: list[str] = field(default_factory=list)
    total_queries: int = 0
    total_duration_ms: float = 0.0


@dataclass
class DisciplineFinding:
    """A cross-request finding emitted by Discipline."""

    code: str
    title: str
    severity: str  # 'info' | 'warning' | 'error'
    message: str
    duplicate_count: int = 0
    time_since_first_ms: float = 0.0
    operation_name: str = ''
    path: str = ''
    client_fp_source: str = ''


# ── The Discipline tracker ────────────────────────────────────────────────────


class Discipline:
    """Cross-request duplicate tracker.

    Maintains a process-scoped, thread-safe ring buffer of recent requests.
    After each request, call :meth:`register` to record it and check for
    duplicates within the configured time window.
    """

    _lock = threading.Lock()
    _traces: deque[RequestTrace] = deque(maxlen=500)

    @classmethod
    def _get_window(cls) -> float:
        return float(getattr(settings, 'GANGSTARR_DISCIPLINE_WINDOW', 5.0))

    @classmethod
    def _get_buffer_size(cls) -> int:
        return int(getattr(settings, 'GANGSTARR_DISCIPLINE_BUFFER_SIZE', 500))

    @classmethod
    def _is_enabled(cls) -> bool:
        return bool(getattr(settings, 'GANGSTARR_DISCIPLINE_ENABLED', True))

    @classmethod
    def register(cls, trace: RequestTrace) -> list[DisciplineFinding]:
        """Register a request trace and return any duplicate findings."""
        if not cls._is_enabled():
            return []

        with cls._lock:
            # Ensure buffer size matches settings (may change between calls).
            max_size = cls._get_buffer_size()
            if cls._traces.maxlen != max_size:
                cls._traces = deque(cls._traces, maxlen=max_size)

            window = cls._get_window()
            cutoff = trace.timestamp - window

            # Prune entries older than the window.
            while cls._traces and cls._traces[0].timestamp < cutoff:
                cls._traces.popleft()

            # Find duplicates: same client + same request fingerprint.
            duplicates = [
                t for t in cls._traces
                if (
                    t.client_fingerprint == trace.client_fingerprint
                    and t.request_fingerprint == trace.request_fingerprint
                )
            ]

            # Always record the trace.
            cls._traces.append(trace)

            if not duplicates:
                return []

            # G010: Duplicate Request detected.
            first = duplicates[0]
            count = len(duplicates) + 1  # including current
            elapsed_ms = (trace.timestamp - first.timestamp) * 1000

            label = trace.operation_name or trace.path
            severity = 'error' if count >= 4 else ('warning' if count >= 2 else 'info')

            finding = DisciplineFinding(
                code='G010',
                title='Duplicate request',
                severity=severity,
                message=(
                    f'{label} fired {count}x in {elapsed_ms:.0f}ms '
                    f'from same client ({trace.client_fp_source})'
                ),
                duplicate_count=count,
                time_since_first_ms=elapsed_ms,
                operation_name=trace.operation_name,
                path=trace.path,
                client_fp_source=trace.client_fp_source,
            )

            logger.info('[G010] %s', finding.message)
            return [finding]

    @classmethod
    def reset(cls) -> None:
        """Clear the ring buffer. Useful for testing."""
        with cls._lock:
            cls._traces.clear()

    @classmethod
    def snapshot(cls) -> list[RequestTrace]:
        """Return a copy of all traces in the buffer. Useful for testing/debug."""
        with cls._lock:
            return list(cls._traces)
