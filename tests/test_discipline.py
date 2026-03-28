"""Tests for gangstarr.discipline — cross-request duplicate tracing."""

import json
import time

import pytest
from django.test import RequestFactory

from gangstarr.discipline import (
    Discipline,
    RequestTrace,
    compute_request_fingerprint,
    hash_graphql_variables,
    resolve_client_fingerprint,
)

# ── Fixtures ──────────────────────────────────────────────────────────────────


@pytest.fixture(autouse=True)
def reset_discipline():
    """Ensure every test starts with a clean ring buffer."""
    Discipline.reset()
    yield
    Discipline.reset()


# ── Client fingerprint waterfall ──────────────────────────────────────────────


class TestResolveClientFingerprint:
    """Priority waterfall: OTEL → correlation headers → IP+UA."""

    def test_otel_traceparent_wins(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            HTTP_TRACEPARENT='00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01',
            HTTP_X_REQUEST_ID='should-not-use-this',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'otel'
        assert fp == '4bf92f3577b34da6a3ce929d0e0e4736'

    def test_x_correlation_id(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            HTTP_X_CORRELATION_ID='abc-123-def',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'x-correlation-id'
        assert fp == 'abc-123-def'

    def test_x_request_id(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            HTTP_X_REQUEST_ID='req-456',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'x-request-id'
        assert fp == 'req-456'

    def test_aws_trace_id(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            HTTP_X_AMZN_TRACE_ID='Root=1-67891233-abcdef012345678912345678',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'x-amzn-trace-id'

    def test_gcp_cloud_trace(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            HTTP_X_CLOUD_TRACE_CONTEXT='105445aa7843bc8bf206b12000100000/1;o=1',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'x-cloud-trace-context'

    def test_b3_trace_id(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            HTTP_X_B3_TRACEID='80f198ee56343ba864fe8b2a57d3eff7',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'x-b3-traceid'

    def test_ip_ua_fallback(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            REMOTE_ADDR='192.168.1.1',
            HTTP_USER_AGENT='Mozilla/5.0 TestBrowser',
        )
        fp, source = resolve_client_fingerprint(request)
        assert source == 'ip+ua'
        assert len(fp) == 24  # sha256 hex truncated to 24

    def test_ip_ua_with_x_forwarded_for(self):
        factory = RequestFactory()
        request = factory.get(
            '/api/test/',
            REMOTE_ADDR='10.0.0.1',
            HTTP_X_FORWARDED_FOR='203.0.113.50, 70.41.3.18',
            HTTP_USER_AGENT='TestBrowser',
        )
        fp1, source = resolve_client_fingerprint(request)
        assert source == 'ip+ua'

        # Same X-Forwarded-For → same fingerprint
        request2 = factory.get(
            '/api/test/',
            REMOTE_ADDR='10.0.0.2',  # different internal IP
            HTTP_X_FORWARDED_FOR='203.0.113.50, 70.41.3.18',
            HTTP_USER_AGENT='TestBrowser',
        )
        fp2, _ = resolve_client_fingerprint(request2)
        assert fp1 == fp2

    def test_ip_ua_differs_by_auth_token(self):
        factory = RequestFactory()
        request1 = factory.get(
            '/api/test/',
            REMOTE_ADDR='192.168.1.1',
            HTTP_USER_AGENT='Same Browser',
            HTTP_AUTHORIZATION='Bearer token-user-a',
        )
        request2 = factory.get(
            '/api/test/',
            REMOTE_ADDR='192.168.1.1',
            HTTP_USER_AGENT='Same Browser',
            HTTP_AUTHORIZATION='Bearer token-user-b',
        )
        fp1, _ = resolve_client_fingerprint(request1)
        fp2, _ = resolve_client_fingerprint(request2)
        assert fp1 != fp2


# ── Request fingerprinting ────────────────────────────────────────────────────


class TestRequestFingerprint:
    def test_same_request_same_fingerprint(self):
        fp1 = compute_request_fingerprint('POST', '/graphql/', 'MyQuery', 'abc')
        fp2 = compute_request_fingerprint('POST', '/graphql/', 'MyQuery', 'abc')
        assert fp1 == fp2

    def test_different_operation_different_fingerprint(self):
        fp1 = compute_request_fingerprint('POST', '/graphql/', 'QueryA', '')
        fp2 = compute_request_fingerprint('POST', '/graphql/', 'QueryB', '')
        assert fp1 != fp2

    def test_different_variables_different_fingerprint(self):
        fp1 = compute_request_fingerprint('POST', '/graphql/', 'MyQuery', 'hash1')
        fp2 = compute_request_fingerprint('POST', '/graphql/', 'MyQuery', 'hash2')
        assert fp1 != fp2

    def test_rest_request_fingerprint(self):
        fp1 = compute_request_fingerprint('GET', '/api/users/', '', '')
        fp2 = compute_request_fingerprint('GET', '/api/users/', '', '')
        assert fp1 == fp2

        fp3 = compute_request_fingerprint('POST', '/api/users/', '', '')
        assert fp1 != fp3


class TestHashGraphqlVariables:
    def test_no_variables(self):
        assert hash_graphql_variables({'query': '{ users { id } }'}) == ''

    def test_none_body(self):
        assert hash_graphql_variables(None) == ''

    def test_with_variables(self):
        body = {'variables': {'id': '123', 'name': 'test'}}
        h = hash_graphql_variables(body)
        assert len(h) == 16
        # Same variables → same hash
        assert hash_graphql_variables(body) == h

    def test_variable_order_independent(self):
        h1 = hash_graphql_variables({'variables': {'a': 1, 'b': 2}})
        h2 = hash_graphql_variables({'variables': {'b': 2, 'a': 1}})
        assert h1 == h2

    def test_different_variables_different_hash(self):
        h1 = hash_graphql_variables({'variables': {'id': '1'}})
        h2 = hash_graphql_variables({'variables': {'id': '2'}})
        assert h1 != h2


# ── Discipline tracker ────────────────────────────────────────────────────────


def _make_trace(
    client_fp='client-A',
    request_fp='request-1',
    timestamp=None,
    **kwargs,
) -> RequestTrace:
    return RequestTrace(
        timestamp=timestamp or time.monotonic(),
        client_fingerprint=client_fp,
        client_fp_source=kwargs.get('client_fp_source', 'ip+ua'),
        request_fingerprint=request_fp,
        request_id='req-001',
        method='POST',
        path='/graphql/',
        operation_name=kwargs.get('operation_name', 'FetchFields'),
        operation_type='query',
    )


class TestDiscipline:
    def test_first_request_no_finding(self):
        trace = _make_trace()
        findings = Discipline.register(trace)
        assert findings == []

    def test_duplicate_request_detected(self):
        t1 = _make_trace()
        Discipline.register(t1)

        t2 = _make_trace()
        findings = Discipline.register(t2)
        assert len(findings) == 1
        assert findings[0].code == 'G010'
        assert findings[0].duplicate_count == 2
        assert 'FetchFields' in findings[0].message

    def test_three_duplicates_escalates_severity(self):
        for _ in range(3):
            Discipline.register(_make_trace())
        findings = Discipline.register(_make_trace())
        assert len(findings) == 1
        assert findings[0].severity == 'error'
        assert findings[0].duplicate_count == 4

    def test_different_client_no_duplicate(self):
        Discipline.register(_make_trace(client_fp='client-A'))
        findings = Discipline.register(_make_trace(client_fp='client-B'))
        assert findings == []

    def test_different_request_fp_no_duplicate(self):
        Discipline.register(_make_trace(request_fp='req-1'))
        findings = Discipline.register(_make_trace(request_fp='req-2'))
        assert findings == []

    def test_time_window_prunes_old_traces(self, settings):
        settings.GANGSTARR_DISCIPLINE_WINDOW = 0.1  # 100ms window

        now = time.monotonic()
        # Old trace — outside window
        old_trace = _make_trace(timestamp=now - 1.0)
        Discipline.register(old_trace)

        # New trace — should NOT see the old one as a duplicate
        new_trace = _make_trace(timestamp=now)
        findings = Discipline.register(new_trace)
        assert findings == []

    def test_disabled_via_setting(self, settings):
        settings.GANGSTARR_DISCIPLINE_ENABLED = False
        Discipline.register(_make_trace())
        findings = Discipline.register(_make_trace())
        assert findings == []

    def test_reset_clears_buffer(self):
        Discipline.register(_make_trace())
        assert len(Discipline.snapshot()) == 1
        Discipline.reset()
        assert len(Discipline.snapshot()) == 0

    def test_snapshot_returns_copy(self):
        Discipline.register(_make_trace())
        snap = Discipline.snapshot()
        assert len(snap) == 1
        # Mutating snapshot should not affect internal buffer
        snap.clear()
        assert len(Discipline.snapshot()) == 1


# ── Middleware integration (GraphQL extraction now returns 3-tuple) ────────────


def test_graphql_extraction_returns_body_data_with_variables():
    """_extract_graphql_info returns body_data so we can hash variables."""
    from gangstarr.middleware import MomentOfTruthMiddleware

    factory = RequestFactory()
    body = json.dumps({
        'query': 'query FetchFields($id: ID!) { field(id: $id) { name } }',
        'operationName': 'FetchFields',
        'variables': {'id': '929d45aa'},
    })
    request = factory.post('/graphql/', data=body, content_type='application/json')
    op_name, op_type, body_data = MomentOfTruthMiddleware._extract_graphql_info(request)
    assert op_name == 'FetchFields'
    assert op_type == 'query'
    assert body_data is not None
    assert body_data['variables'] == {'id': '929d45aa'}

    # Variables hash should be deterministic
    h = hash_graphql_variables(body_data)
    assert len(h) == 16
