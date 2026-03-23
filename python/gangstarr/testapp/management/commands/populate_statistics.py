"""populate_statistics — generate enough DB traffic to exercise gangstarr profiling.

Loads the chinook fixture, then hammers every known endpoint (REST, GraphQL,
Django views) with concurrent requests so that MomentOfTruthMiddleware and
full_clip capture realistic N+1 and over-fetch patterns.

If no dev server is already running, one is started automatically and torn
down when the command finishes.

Usage:
    python manage.py populate_statistics [--rounds 20] [--concurrency 4] [--host localhost:8000]
"""

import json
import os
import random
import subprocess
import sys
import time
import urllib.request
import urllib.error
import warnings
from concurrent.futures import ThreadPoolExecutor, as_completed

from django.core.management import call_command
from django.core.management.base import BaseCommand


ENDPOINTS = [
    "/",
    "/artists/",
    "/api/artists/",
]

GRAPHQL_QUERIES = [
    '{"query": "{ allArtists(limit: 50) { id name albums { id title } } }"}',
    '{"query": "{ artistsWithAlbumsAndTracks(limit: 20) { id name albums { id title tracks { id name } } } }"}',
]


def _server_is_up(base_url):
    """Return True if the dev server is accepting connections."""
    try:
        urllib.request.urlopen(f"{base_url}/", timeout=2)
        return True
    except Exception:
        return False


def _wait_for_server(base_url, timeout=15):
    """Poll until the server responds or *timeout* seconds elapse."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if _server_is_up(base_url):
            return True
        time.sleep(0.5)
    return False


def _hit(base_url, path, body=None):
    url = f"{base_url}{path}"
    headers = {"Content-Type": "application/json"} if body else {}
    data = body.encode() if body else None
    req = urllib.request.Request(url, data=data, headers=headers)
    try:
        with urllib.request.urlopen(req, timeout=10) as resp:
            return resp.status
    except urllib.error.HTTPError as e:
        return e.code
    except Exception as e:
        return str(e)


class Command(BaseCommand):
    help = "Load chinook data and generate traffic to exercise gangstarr profiling"

    def add_arguments(self, parser):
        parser.add_argument("--rounds", type=int, default=20, help="Number of request rounds")
        parser.add_argument("--concurrency", type=int, default=4, help="Concurrent workers")
        parser.add_argument("--host", default="localhost:8000", help="Dev server host:port")
        parser.add_argument("--skip-fixture", action="store_true", help="Skip loading chinook fixture")

    def handle(self, *args, **options):
        if not options["skip_fixture"]:
            self.stdout.write("Loading chinook fixture...")
            with warnings.catch_warnings():
                warnings.filterwarnings("ignore", message=".*received a naive datetime.*")
                call_command("loaddata", "chinook", verbosity=0)
            self.stdout.write(self.style.SUCCESS("Chinook data loaded."))

        # Build artist detail URLs from the DB
        from gangstarr.testapp.models import Artist
        artist_ids = list(Artist.objects.values_list("id", flat=True)[:50])
        detail_paths = [f"/artists/{aid}/" for aid in artist_ids]

        base_url = f"http://{options['host']}"
        rounds = options["rounds"]
        concurrency = options["concurrency"]

        # Auto-start a dev server if one isn't already running
        server_proc = None
        if not _server_is_up(base_url):
            self.stdout.write("No dev server detected — starting one in the background...")
            server_proc = subprocess.Popen(
                [
                    sys.executable,
                    "python/gangstarr/testapp/manage.py",
                    "runserver", options["host"],
                    "--noreload",
                ],
                env=os.environ.copy(),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            if not _wait_for_server(base_url):
                server_proc.kill()
                self.stderr.write(self.style.ERROR(
                    f"Dev server failed to start on {base_url}. "
                    "Try running 'make runserver' manually and then re-run this command."
                ))
                return
            self.stdout.write(self.style.SUCCESS(f"Dev server ready on {base_url}"))

        try:
            tasks = []
            for _ in range(rounds):
                for ep in ENDPOINTS:
                    tasks.append((ep, None))
                for gql in GRAPHQL_QUERIES:
                    tasks.append(("/graphql/", gql))
                if detail_paths:
                    tasks.append((random.choice(detail_paths), None))

            self.stdout.write(f"Firing {len(tasks)} requests at {base_url} (concurrency={concurrency})...")

            ok = 0
            err = 0
            with ThreadPoolExecutor(max_workers=concurrency) as pool:
                futures = {pool.submit(_hit, base_url, path, body): path for path, body in tasks}
                for f in as_completed(futures):
                    result = f.result()
                    if isinstance(result, int) and 200 <= result < 400:
                        ok += 1
                    else:
                        err += 1

            self.stdout.write(self.style.SUCCESS(f"Done. {ok} ok / {err} errors out of {len(tasks)} requests."))
        finally:
            if server_proc:
                self.stdout.write("Stopping background dev server...")
                server_proc.terminate()
                try:
                    server_proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    server_proc.kill()
                self.stdout.write("Dev server stopped.")
