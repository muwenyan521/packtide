#!/usr/bin/env python3
import json
import os
import subprocess
import sys

tag = os.environ.get("GITHUB_REF_NAME", "")
expected = tag[1:] if tag.startswith("v") else tag
if not expected:
    raise SystemExit("GITHUB_REF_NAME is required")
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--no-deps", "--format-version", "1"], text=True))
packages = {package["name"]: package["version"] for package in metadata["packages"]}
for name in ("packtide", "systide"):
    actual = packages.get(name)
    if actual != expected:
        raise SystemExit(f"{name} version {actual!r} does not match tag {expected!r}")
print(f"release version {expected} matches packtide and systide")
