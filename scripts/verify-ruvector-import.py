#!/usr/bin/env python3
"""Verify the committed RuVector snapshot using Git object IDs, without a build."""

import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT)


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def main():
    require(len(sys.argv) <= 2, "usage: verify-ruvector-import.py [commit-or-tree]")
    ref = sys.argv[1] if len(sys.argv) == 2 else "HEAD"
    # Resolve the revision before using it as a path-bearing Git object expression.
    ref = git("rev-parse", "--verify", "--end-of-options", ref + "^{tree}").decode().strip()
    manifest = json.loads(git("show", f"{ref}:upstream/ruvector-import.json"))
    prefix = manifest["path"]
    actual = git("rev-parse", f"{ref}:{prefix}").decode().strip()
    require(actual == manifest["tree"], f"RuVector tree mismatch: {actual}")

    blobs = 0
    blob_bytes = 0
    gitlinks = {}
    for entry in git("ls-tree", "-rlz", actual).split(b"\0"):
        if not entry:
            continue
        fields, path = entry.split(b"\t", 1)
        mode, kind, sha, size = fields.split()
        if kind == b"blob":
            blobs += 1
            blob_bytes += int(size)
        elif kind == b"commit":
            require(mode == b"160000", "Unexpected commit entry mode")
            gitlinks[path.decode()] = sha.decode()
        else:
            raise SystemExit(f"Unexpected source entry: {entry!r}")

    require(blobs == manifest["tracked_blobs"], "Source file count mismatch")
    require(blob_bytes == manifest["tracked_blob_bytes"], "Source byte count mismatch")
    expected = {item["path"]: item["commit"] for item in manifest["submodules"]}
    require(gitlinks == expected, "External submodule pins differ from the import record")
    for item in manifest["submodules"]:
        path = f"{prefix}/{item['path']}"
        for key, value in (("path", path), ("url", item["repository"])):
            actual_value = git(
                "config", f"--blob={ref}:.gitmodules", "--get", f"submodule.{path}.{key}"
            ).decode().strip()
            require(actual_value == value, f"Submodule {key} mismatch for {path}")

    print(f"Verified RuVector tree {actual}: {blobs} blobs, {blob_bytes} bytes")
    print(f"Verified {len(gitlinks)} pinned submodule mappings")


if __name__ == "__main__":
    main()
