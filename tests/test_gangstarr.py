import json

import pytest
from django.test import Client
from django.urls import reverse

from gangstarr import full_clip
from gangstarr.engine import analyze, fingerprint_sql, normalize_sql
from gangstarr.reporting import LoggingOptions, MassAppealException, RaisingOptions
from gangstarr.schemas import QueryEvent
from gangstarr.testapp.my_module import create_albums, get_artists

# --- Existing tests (backward compat) ---


@pytest.mark.django_db(transaction=True)
def test_full_clip():
    create_albums()
    with full_clip(meta_data=dict(func='get_artists', username='Paul')) as fc:
        get_artists()
    query_info = fc.query_info
    assert len(query_info) == 1


@pytest.mark.django_db(transaction=True)
def test_moment_of_truth_middleware():
    create_albums()
    client = Client()
    response = client.get(reverse('artists'))
    assert response.status_code == 200


@pytest.mark.django_db(transaction=True)
def test_logging():
    create_albums()
    with full_clip(reporting_options=LoggingOptions()) as fc:
        get_artists()
    query_info = fc.query_info
    assert len(query_info) == 1


@pytest.mark.django_db(transaction=True)
def test_mass_appeal_exception():
    create_albums()
    with pytest.raises(MassAppealException):
        with full_clip(reporting_options=RaisingOptions(count_threshold=1, duration_threshold=0.0)):
            get_artists()


# --- Rust engine tests ---


def test_normalize_sql():
    result = normalize_sql("SELECT * FROM song WHERE id = 1")
    assert "$" in result
    # Different literal values should produce the same normalized form
    result2 = normalize_sql("SELECT * FROM song WHERE id = 42")
    assert result == result2


def test_fingerprint_sql():
    fp1 = fingerprint_sql("SELECT * FROM song WHERE id = 1")
    fp2 = fingerprint_sql("SELECT * FROM song WHERE id = 2")
    # Same shape → same fingerprint
    assert fp1 == fp2
    assert len(fp1) > 0

    # Different shape → different fingerprint
    fp3 = fingerprint_sql("SELECT * FROM artist WHERE name = 'test'")
    assert fp1 != fp3


def test_analyze_events():
    events = [
        QueryEvent(
            sql='SELECT "testapp_artist"."id", "testapp_artist"."name" FROM "testapp_artist"',
            duration_ms=0.5,
            file='testapp/views.py',
            line=10,
            function='artist_list',
            source='artists = Artist.objects.all()',
        ),
        QueryEvent(
            sql='SELECT "testapp_artist"."id", "testapp_artist"."name" FROM "testapp_artist"',
            duration_ms=0.3,
            file='testapp/views.py',
            line=10,
            function='artist_list',
            source='artists = Artist.objects.all()',
        ),
    ]
    result = analyze(events)

    assert 'summary' in result
    assert 'groups' in result
    assert 'findings' in result
    assert result['summary']['total_queries'] == 2
    assert result['summary']['unique_queries'] == 1
    assert result['summary']['reads'] == 2
    assert result['summary']['writes'] == 0


def test_analyze_detects_duplicates():
    events = [
        QueryEvent(
            sql=f'SELECT * FROM artist WHERE id = {i}',
            duration_ms=0.1,
            file='views.py',
            line=5,
            function='get_artist',
            source='Artist.objects.get(pk=i)',
        )
        for i in range(10)
    ]
    result = analyze(events)

    # Should detect duplicates (same fingerprint, different literal values)
    finding_codes = [f['code'] for f in result['findings']]
    assert 'G001' in finding_codes  # duplicate queries
    assert 'G002' in finding_codes  # N+1 pattern (same callsite)


# --- Integration: events are collected during full_clip ---


@pytest.mark.django_db(transaction=True)
def test_full_clip_collects_events():
    create_albums()
    with full_clip() as fc:
        get_artists()
    # New: events should be collected
    assert len(fc._premier.events) > 0
    # Each event should have the expected fields
    event = fc._premier.events[0]
    assert event.sql
    assert event.file
    assert event.line > 0


# --- Path filtering tests ---


def test_middleware_excludes_static_paths():
    from gangstarr.middleware import MomentOfTruthMiddleware

    mw = MomentOfTruthMiddleware(get_response=lambda r: r)
    assert mw._is_excluded('/static/js/app.js') is True
    assert mw._is_excluded('/favicon.ico') is True
    assert mw._is_excluded('/media/uploads/photo.jpg') is True
    assert mw._is_excluded('/__debug__/') is True
    assert mw._is_excluded('/api/artists/') is False
    assert mw._is_excluded('/graphql/') is False


def test_middleware_custom_exclude_paths(settings):
    from gangstarr.middleware import MomentOfTruthMiddleware

    settings.GANGSTARR_EXCLUDE_PATHS = ['/health/', '/metrics/']
    mw = MomentOfTruthMiddleware(get_response=lambda r: r)
    assert mw._is_excluded('/health/') is True
    assert mw._is_excluded('/metrics/') is True
    # Default exclusions should NOT apply with custom list
    assert mw._is_excluded('/static/js/app.js') is False


# --- GraphQL operation extraction tests ---


def test_graphql_extraction_json_body():
    from django.test import RequestFactory

    from gangstarr.middleware import MomentOfTruthMiddleware

    factory = RequestFactory()
    body = json.dumps({
        'query': 'query MyQuery { artists { id name } }',
        'operationName': 'MyQuery',
    })
    request = factory.post(
        '/graphql/',
        data=body,
        content_type='application/json',
    )
    op_name, op_type, body = MomentOfTruthMiddleware._extract_graphql_info(request)
    assert op_name == 'MyQuery'
    assert op_type == 'query'
    assert body is not None


def test_graphql_extraction_mutation():
    from django.test import RequestFactory

    from gangstarr.middleware import MomentOfTruthMiddleware

    factory = RequestFactory()
    body = json.dumps({
        'query': 'mutation CreateArtist($name: String!) { createArtist(name: $name) { id } }',
    })
    request = factory.post('/graphql/', data=body, content_type='application/json')
    op_name, op_type, _ = MomentOfTruthMiddleware._extract_graphql_info(request)
    assert op_name == 'CreateArtist'
    assert op_type == 'mutation'


def test_graphql_extraction_no_operation_name():
    from django.test import RequestFactory

    from gangstarr.middleware import MomentOfTruthMiddleware

    factory = RequestFactory()
    body = json.dumps({'query': '{ artists { id } }'})
    request = factory.post('/graphql/', data=body, content_type='application/json')
    op_name, op_type, _ = MomentOfTruthMiddleware._extract_graphql_info(request)
    # Anonymous query — no named operation
    assert op_name == ''
    assert op_type == ''


def test_graphql_extraction_get_request_skipped():
    from django.test import RequestFactory

    from gangstarr.middleware import MomentOfTruthMiddleware

    factory = RequestFactory()
    request = factory.get('/graphql/')
    op_name, op_type, body = MomentOfTruthMiddleware._extract_graphql_info(request)
    assert op_name == ''
    assert op_type == ''
    assert body is None


# --- Resolver index tests ---


def test_camel_to_snake():
    from gangstarr.gangstarr import camel_to_snake

    assert camel_to_snake('artistsWithAlbumsAndTracks') == 'artists_with_albums_and_tracks'
    assert camel_to_snake('allArtists') == 'all_artists'
    assert camel_to_snake('id') == 'id'
    assert camel_to_snake('__typename') == '__typename'
    assert camel_to_snake('unitPrice') == 'unit_price'
    assert camel_to_snake('already_snake') == 'already_snake'


def test_scan_resolvers_with_testapp_schema():
    import json
    from pathlib import Path

    from gangstarr.gangstarr import scan_resolvers

    schema_path = Path(__file__).resolve().parent.parent / 'python' / 'gangstarr' / 'testapp' / 'schema.py'
    content = schema_path.read_text()
    files_json = json.dumps([{'path': 'testapp/schema.py', 'content': content}])
    raw = json.loads(scan_resolvers(files_json))

    # Explicit resolvers
    assert 'Query.all_artists' in raw
    assert raw['Query.all_artists']['kind'] == 'explicit'
    assert 'resolve_all_artists' in raw['Query.all_artists']['source']

    assert 'Query.artists_with_albums_and_tracks' in raw

    # Implicit fields from DjangoObjectType
    assert 'ArtistType.albums' in raw
    assert raw['ArtistType.albums']['kind'] == 'implicit'

    assert 'AlbumType.tracks' in raw
    assert 'AlbumType.artist' in raw


def test_resolver_index_lookup():
    """ResolverIndex camelCase→snake_case lookup works."""
    from django.conf import settings

    from gangstarr.resolver_index import ResolverIndex

    idx = ResolverIndex(settings.GANGSTAR_BASE_DIR)
    # Direct snake_case lookup
    loc = idx.lookup('Query.all_artists')
    assert loc is not None
    assert loc.kind == 'explicit'
    assert 'schema.py' in loc.file

    # camelCase → snake_case lookup
    loc = idx.lookup('Query.artistsWithAlbumsAndTracks')
    assert loc is not None
    assert 'schema.py' in loc.file

    # Implicit field lookup
    loc = idx.lookup('ArtistType.albums')
    assert loc is not None
    assert loc.kind == 'implicit'

    # Non-existent
    assert idx.lookup('FakeType.nonexistent') is None


# --- Report header with GraphQL info ---


def test_report_header_includes_graphql_operation():
    from gangstarr.reporting import _format_report
    from gangstarr.schemas import RequestContext

    ctx = RequestContext(
        method='POST',
        path='/graphql/',
        view_name='GraphQLView',
        operation_name='MyQuery',
        operation_type='query',
    )
    analysis = {
        'summary': {
            'total_queries': 10,
            'unique_queries': 2,
            'total_duration_ms': 5.0,
            'duplicate_groups': 1,
            'reads': 10,
            'writes': 0,
        },
        'groups': [],
        'findings': [],
    }
    output = _format_report(analysis, ctx)
    assert 'GraphQLView' in output
    assert 'GRAPHQL OPERATION' in output
    assert 'query MyQuery' in output


# --- Integration tests: full request/response cycle ---


@pytest.fixture
def seed_data(db):
    """Create test data with artists, albums, and tracks."""
    from gangstarr.testapp.models import Album, Artist, MediaType, Track

    media_type = MediaType.objects.create(name='MPEG')
    for i in range(3):
        artist = Artist.objects.create(name=f'Artist {i}')
        for j in range(2):
            album = Album.objects.create(title=f'Album {i}-{j}', artist=artist)
            for k in range(2):
                Track.objects.create(
                    name=f'Track {i}-{j}-{k}',
                    album=album,
                    media_type=media_type,
                    milliseconds=240000,
                    unit_price=0.99,
                )


@pytest.fixture
def capture_events(settings):
    """Intercept Guru._run_analysis to capture events from middleware-profiled requests.

    Django's test framework sets DEBUG=False by default, which causes
    MomentOfTruthMiddleware to skip profiling.  We re-enable it here.
    """
    from unittest import mock

    import gangstarr.resolver_index as ri
    from gangstarr.reporting import Guru

    settings.DEBUG = True
    # Reset the resolver index singleton so tests get a fresh scan
    ri._singleton = None

    all_events = []
    all_contexts = []
    original_run = Guru._run_analysis

    def capturing_run(self):
        all_events.extend(self.premier.events)
        all_contexts.append(self.premier.request_context)
        return original_run(self)

    with mock.patch.object(Guru, '_run_analysis', capturing_run):
        yield {'events': all_events, 'contexts': all_contexts}


@pytest.mark.django_db(transaction=True)
def test_integration_home_page(seed_data, capture_events):
    """Home page: COUNT query attributed to views.py / home."""
    client = Client()
    response = client.get('/')
    assert response.status_code == 200

    events = capture_events['events']
    assert len(events) >= 1

    # The COUNT query should be attributed to views.py
    count_event = next((e for e in events if 'COUNT' in e.sql.upper()), None)
    assert count_event is not None
    assert 'views.py' in count_event.file
    assert count_event.function == 'home'


@pytest.mark.django_db(transaction=True)
def test_integration_drf_n_plus_one(seed_data, capture_events):
    """DRF artists list: N+1 from serializer accessing artist.albums."""
    client = Client()
    response = client.get('/api/artists/')
    assert response.status_code == 200

    events = capture_events['events']
    # 1 query for Artist.objects.all() + N for album access via serializer
    assert len(events) > 1

    # All events should have file attribution (no empty files)
    for event in events:
        assert event.file, f'Event missing file attribution: {event.sql[:80]}'

    # The initial artist query should come from api_views.py
    files_seen = {e.file for e in events}
    assert any('api_views' in f for f in files_seen)


@pytest.mark.django_db(transaction=True)
def test_integration_graphql_resolver_attribution(seed_data, capture_events):
    """GraphQL N+1: resolver_path from DWYCKMiddleware attributes queries to resolvers."""
    client = Client()
    query = '{ artistsWithAlbumsAndTracks(limit: 3) { id name albums { id title tracks { id name } } } }'
    response = client.post(
        '/graphql/',
        data=json.dumps({'query': query}),
        content_type='application/json',
    )
    assert response.status_code == 200
    data = json.loads(response.content)
    assert 'errors' not in data

    events = capture_events['events']
    # Should have: 1 artist query + 3 album queries + 6 track queries (N+1)
    assert len(events) >= 4

    # N+1 queries should have resolver_path from DWYCKMiddleware.
    # The initial artist query is a lazy queryset evaluated by Graphene's
    # executor after the resolver middleware exits, so it won't have a
    # resolver_path.  But the N+1 album/track queries are triggered inside
    # sub-field resolvers where DWYCKMiddleware is active.
    resolver_events = [e for e in events if e.resolver_path]
    assert len(resolver_events) > 0, (
        f'Expected resolver_path on N+1 events, got paths: '
        f'{[(e.file, e.resolver_path) for e in events]}'
    )

    resolver_paths = {e.resolver_path for e in resolver_events}
    # Should see ArtistType.albums (lazy album fetches per artist)
    assert any('ArtistType' in rp for rp in resolver_paths), (
        f'Expected ArtistType resolver, got: {resolver_paths}'
    )

    # Resolver index should remap file attribution from middleware.py to schema.py
    album_events = [e for e in resolver_events if 'ArtistType' in e.resolver_path]
    for e in album_events:
        assert 'schema.py' in e.file, (
            f'Expected schema.py attribution, got {e.file} for {e.resolver_path}'
        )


@pytest.mark.django_db(transaction=True)
def test_integration_graphql_optimized_query(seed_data, capture_events):
    """GraphQL allArtists uses prefetch_related — fewer queries than N+1."""
    client = Client()
    query = '{ allArtists(limit: 3) { id name albums { id title } } }'
    response = client.post(
        '/graphql/',
        data=json.dumps({'query': query}),
        content_type='application/json',
    )
    assert response.status_code == 200

    events = capture_events['events']
    # prefetch_related should result in only 2 queries (artists + albums)
    # instead of N+1
    assert len(events) <= 4, f'Expected <=3 queries with prefetch, got {len(events)}'
