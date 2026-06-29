#!/usr/bin/env python3
"""Generate a dependency-license inventory outside the source tree.

Usage:
  python3 tools/inventory-third-party-licenses.py --output-dir /tmp/accordmesh-license-audit

The script does not download dependencies. Run it after `pnpm install` and with
Cargo available. Missing or empty license metadata is reported for review.
"""

from __future__ import annotations

import argparse
import csv
import json
import subprocess
from pathlib import Path


def cargo_inventory(root: Path) -> list[dict[str, str]]:
    command = [
        "cargo",
        "metadata",
        "--format-version",
        "1",
        "--locked",
        "--manifest-path",
        str(root / "apps/desktop/src-tauri/Cargo.toml"),
    ]
    payload = json.loads(subprocess.check_output(command, text=True))
    rows = []
    for package in payload.get("packages", []):
        rows.append(
            {
                "ecosystem": "cargo",
                "name": package.get("name", ""),
                "version": package.get("version", ""),
                "license": package.get("license") or "",
                "license_file": package.get("license_file") or "",
                "source": package.get("source") or "workspace/local",
                "manifest": package.get("manifest_path") or "",
            }
        )
    return rows


def npm_inventory(root: Path) -> list[dict[str, str]]:
    package_files = set()
    for pattern in (
        "node_modules/*/package.json",
        "node_modules/@*/*/package.json",
        "node_modules/.pnpm/*/node_modules/*/package.json",
        "node_modules/.pnpm/*/node_modules/@*/*/package.json",
    ):
        package_files.update(root.glob(pattern))

    rows = []
    seen = set()
    for package_file in sorted(package_files):
        try:
            package = json.loads(package_file.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue
        name = str(package.get("name", ""))
        version = str(package.get("version", ""))
        if not name or not version or (name, version) in seen:
            continue
        seen.add((name, version))
        license_value = package.get("license", "")
        if isinstance(license_value, dict):
            license_value = license_value.get("type", "")
        rows.append(
            {
                "ecosystem": "npm",
                "name": name,
                "version": version,
                "license": str(license_value or ""),
                "license_file": "",
                "source": str(package.get("repository", "")),
                "manifest": str(package_file.relative_to(root)),
            }
        )
    return rows


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()

    root = Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    if root == output or root in output.parents:
        raise SystemExit("Output directory must be outside the repository")
    output.mkdir(parents=True, exist_ok=True)

    rows = cargo_inventory(root) + npm_inventory(root)
    rows.sort(key=lambda row: (row["ecosystem"], row["name"].lower(), row["version"]))

    fields = ["ecosystem", "name", "version", "license", "license_file", "source", "manifest"]
    with (output / "dependency_licenses.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(rows)

    missing = [row for row in rows if not row["license"] and not row["license_file"]]
    with (output / "missing_license_metadata.csv").open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fields)
        writer.writeheader()
        writer.writerows(missing)

    summary = {
        "dependency_count": len(rows),
        "cargo_count": sum(row["ecosystem"] == "cargo" for row in rows),
        "npm_count": sum(row["ecosystem"] == "npm" for row in rows),
        "missing_license_metadata_count": len(missing),
        "status": "REVIEW_REQUIRED" if missing else "INVENTORY_COMPLETE_REVIEW_STILL_REQUIRED",
    }
    (output / "SUMMARY.json").write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))
    return 1 if missing else 0


if __name__ == "__main__":
    raise SystemExit(main())
