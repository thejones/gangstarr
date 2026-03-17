"""field_tracker — records which serializer fields are actually returned per
endpoint, feeding concrete data into the G102 field-narrowing analysis.

Named in the Gang Starr tradition: field intelligence helps the Guru give
precise .only() recommendations instead of generic warnings.

Usage
-----
1. Add ``FieldUsageMiddleware`` to ``MIDDLEWARE`` (before DRF view middleware):

       MIDDLEWARE = [
           ...
           "gangstarr.field_tracker.FieldUsageMiddleware",
           ...
       ]

2. Mix ``FieldUsageTrackerMixin`` into your ModelSerializers:

       class BookSerializer(FieldUsageTrackerMixin, serializers.ModelSerializer):
           ...

3. At the end of a request cycle (or in a periodic task), flush and persist:

       from gangstarr.field_tracker import flush_field_usage
       from gangstarr import engine

       records = flush_field_usage()
       engine.store_field_usage_records(records, project_root=BASE_DIR, run_id=run_id)
"""
from __future__ import annotations

import threading
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from django.http import HttpRequest

# ── Thread-local request storage ─────────────────────────────────────────────

_local: threading.local = threading.local()

# ── Field-usage buffer ────────────────────────────────────────────────────────

# Each entry: {model: str, field: str, endpoint: str, serializer: str}
_buffer: list[dict[str, str]] = []
_buffer_lock = threading.Lock()


def _current_endpoint() -> str:
    """Return the URL path of the current request, if any."""
    req: HttpRequest | None = getattr(_local, "request", None)
    if req is None:
        return ""
    return getattr(req, "path", "")


def _infer_model_name(serializer: Any) -> str:
    """Best-effort model name from a serializer instance.

    Reads ``serializer.Meta.model.__name__`` when available, falling back to
    the serializer class name itself.
    """
    try:
        meta = serializer.Meta  # type: ignore[attr-defined]
        model = getattr(meta, "model", None)
        if model is not None:
            return model.__name__
    except AttributeError:
        pass
    return type(serializer).__name__


# ── Mixin ─────────────────────────────────────────────────────────────────────


class FieldUsageTrackerMixin:
    """DRF serializer mixin that records which fields are returned per request.

    Overrides ``to_representation`` to append each returned field key to the
    module-level buffer.  The buffer is drained by ``flush_field_usage()``.

    Add *before* the base serializer class in the MRO so that the override
    fires after Django REST Framework builds the representation::

        class BookSerializer(FieldUsageTrackerMixin, serializers.ModelSerializer):
            class Meta:
                model = Book
                fields = "__all__"
    """

    def to_representation(self, instance: Any) -> Any:  # type: ignore[override]
        result = super().to_representation(instance)  # type: ignore[misc]
        if result:
            model_name = _infer_model_name(self)
            endpoint = _current_endpoint()
            serializer_name = type(self).__name__
            with _buffer_lock:
                for field_name in result:
                    _buffer.append(
                        {
                            "model": model_name,
                            "field": field_name,
                            "endpoint": endpoint,
                            "serializer": serializer_name,
                        }
                    )
        return result


# ── Middleware ────────────────────────────────────────────────────────────────


class FieldUsageMiddleware:
    """Django middleware that exposes the current request to ``FieldUsageTrackerMixin``.

    Place this in ``MIDDLEWARE`` *before* any view middleware so that the
    request path is available when serializers run::

        MIDDLEWARE = [
            ...
            "gangstarr.field_tracker.FieldUsageMiddleware",
            ...
        ]
    """

    def __init__(self, get_response: Any) -> None:
        self.get_response = get_response

    def __call__(self, request: Any) -> Any:
        _local.request = request
        try:
            return self.get_response(request)
        finally:
            _local.request = None  # type: ignore[assignment]


# ── Buffer drain ──────────────────────────────────────────────────────────────


def flush_field_usage() -> list[dict[str, str]]:
    """Drain and return the accumulated field-usage buffer.

    Thread-safe.  Typically called at the end of a request or test run, then
    passed directly to ``engine.store_field_usage_records()``:

        records = flush_field_usage()
        engine.store_field_usage_records(records, project_root=BASE_DIR, run_id=run_id)

    Returns a list of dicts with keys: ``model``, ``field``, ``endpoint``,
    ``serializer``.
    """
    with _buffer_lock:
        records = list(_buffer)
        _buffer.clear()
    return records
