# rmtx compatibility contract (Phase 0)

Machine-readable reference extracted from this MediaMTX tree. Everything after Phase 0 treats these files as the drop-in compatibility contract.

| File | Purpose |
|---|---|
| `openapi.yaml` | Full Control API (copy of repo-root `api/openapi.yaml`) |
| `control-api-routes.json` | Flat route index (method + path + operationId) |
| `mediamtx-config.schema.json` | JSON Schema for `mediamtx.yml` (from OpenAPI `GlobalConf` + `PathConf`) |
| `hooks-env.json` | Hook config keys, restart flags, and env-var contract |
| `gui-api-surface.json` | Endpoints the existing `gui/` actually calls |

## Regenerating

From repo root (requires PyYAML):

```bash
python3 -m venv /tmp/rmtx-venv && /tmp/rmtx-venv/bin/pip install pyyaml
# re-run the extractor in scripts/extract-spec.py once added, or copy api/openapi.yaml
cp api/openapi.yaml rmtx/spec/openapi.yaml
```

## Known naming deltas vs the project brief

The brief mentions `runOnPublish` / `runOnUnpublish`. Upstream MediaMTX uses:

- `runOnReady` / `runOnNotReady` — stream readiness
- `runOnSourceConnect` / `runOnSourceDisconnect` — publisher/source attach

rmtx should implement the **upstream** names.
