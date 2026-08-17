#!/usr/bin/env python3
"""Extract rmtx Phase 0 compatibility artifacts from MediaMTX sources.

Run from repo root:
  python3 -m venv /tmp/rmtx-venv && /tmp/rmtx-venv/bin/pip install pyyaml
  /tmp/rmtx-venv/bin/python rmtx/scripts/extract-spec.py
"""

from __future__ import annotations

import json
import pathlib
import shutil
import sys

try:
    import yaml
except ImportError:
    print("PyYAML required: pip install pyyaml", file=sys.stderr)
    sys.exit(1)

ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = ROOT / "rmtx" / "spec"


def convert(node):
    if isinstance(node, dict):
        out = {}
        for k, v in node.items():
            if k == "$ref" and isinstance(v, str) and v.startswith("#/components/schemas/"):
                name = v.rsplit("/", 1)[-1]
                out[k] = f"#/$defs/{name}"
            else:
                out[k] = convert(v)
        if out.pop("nullable", False):
            base = {kk: vv for kk, vv in out.items() if kk != "description"}
            desc = out.get("description")
            wrapped = {"anyOf": [base, {"type": "null"}]}
            if desc:
                wrapped["description"] = desc
            return wrapped
        return out
    if isinstance(node, list):
        return [convert(x) for x in node]
    return node


def main() -> None:
    SPEC.mkdir(parents=True, exist_ok=True)
    openapi_src = ROOT / "api" / "openapi.yaml"
    openapi = yaml.safe_load(openapi_src.read_text())
    schemas = openapi["components"]["schemas"]

    needed: set[str] = set()

    def walk_refs(obj) -> None:
        if isinstance(obj, dict):
            ref = obj.get("$ref")
            if isinstance(ref, str) and ref.startswith("#/components/schemas/"):
                name = ref.rsplit("/", 1)[-1]
                if name not in needed:
                    needed.add(name)
                    walk_refs(schemas[name])
            for v in obj.values():
                walk_refs(v)
        elif isinstance(obj, list):
            for v in obj:
                walk_refs(v)

    for n in ("GlobalConf", "PathConf"):
        needed.add(n)
        walk_refs(schemas[n])

    defs = {name: convert(schemas[name]) for name in sorted(needed)}
    global_props = defs["GlobalConf"].get("properties", {})

    config_schema = {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://github.com/initial-commit-hq/mediamtx/rmtx/spec/mediamtx-config.schema.json",
        "title": "MediaMTX configuration file (mediamtx.yml)",
        "description": (
            "Compatibility contract for rmtx Phase 0. Derived from api/openapi.yaml "
            "GlobalConf + PathConf schemas (the Control API's view of config). "
            "YAML duration/size values are strings (e.g. '10s', '50M') matching OpenAPI."
        ),
        "type": "object",
        "properties": {
            **global_props,
            "pathDefaults": {"$ref": "#/$defs/PathConf"},
            "paths": {
                "type": "object",
                "additionalProperties": {"$ref": "#/$defs/PathConf"},
                "description": (
                    "Map of path name -> path configuration. "
                    "Keys may be literal names or regex patterns."
                ),
            },
        },
        "additionalProperties": False,
        "$defs": {k: v for k, v in defs.items() if k != "GlobalConf"},
    }
    (SPEC / "mediamtx-config.schema.json").write_text(
        json.dumps(config_schema, indent=2) + "\n"
    )

    routes = []
    for path, methods in openapi["paths"].items():
        for method, op in methods.items():
            if method.startswith("x-") or not isinstance(op, dict):
                continue
            routes.append(
                {
                    "method": method.upper(),
                    "path": path,
                    "operationId": op.get("operationId"),
                    "summary": op.get("summary"),
                    "tags": op.get("tags", []),
                }
            )
    (SPEC / "control-api-routes.json").write_text(
        json.dumps(
            {
                "source": "api/openapi.yaml",
                "baseUrlDefault": "http://localhost:9997",
                "routeCount": len(routes),
                "routes": routes,
            },
            indent=2,
        )
        + "\n"
    )

    shutil.copy2(openapi_src, SPEC / "openapi.yaml")
    print(f"updated {SPEC} ({len(routes)} routes, {len(defs)} schema defs)")


if __name__ == "__main__":
    main()
