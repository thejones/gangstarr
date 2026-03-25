#!/usr/bin/env python
"""Dev server with auto Rust rebuild on .rs file changes."""
import subprocess
import sys
import threading

from watchfiles import watch


def rebuild_rust():
    print("\n🔨 Rust change detected, rebuilding...")
    result = subprocess.run(["maturin", "develop"], capture_output=True, text=True)
    if result.returncode == 0:
        print("✅ Rust rebuild complete")
    else:
        print(f"❌ Rust build failed:\n{result.stderr}")


def watch_rust():
    for _changes in watch("src", watch_filter=lambda _, path: path.endswith(".rs")):
        rebuild_rust()


def main():
    # Initial Rust build
    rebuild_rust()

    # Watch Rust files in background
    watcher = threading.Thread(target=watch_rust, daemon=True)
    watcher.start()

    # Run Django dev server (handles its own Python reload)
    subprocess.run(
        [sys.executable, "python/gangstarr/testapp/manage.py", "runserver", "8001"],
    )


if __name__ == "__main__":
    main()
