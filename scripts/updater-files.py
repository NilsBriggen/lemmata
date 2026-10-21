#!/usr/bin/env python3
"""Prints "<file> <signature>" for each platform in an updater manifest (latest.json), for the
release workflow's pre-publish check. Fails if two platforms share a file."""

import json
import sys

platforms = json.load(open(sys.argv[1]))["platforms"]
files = [p["url"].rsplit("/", 1)[1] for p in platforms.values()]
if len(set(files)) != len(files):
    sys.exit(f"platforms share a file: {files}")
for f, p in zip(files, platforms.values()):
    print(f, p["signature"])
