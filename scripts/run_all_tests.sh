#!/usr/bin/env bash
# Run the full agent-workbench test matrix. Exit non-zero on first failure.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "== [1/6] Rust build + tests =="
cargo build --bin agent-workbench
cargo test

echo "== [2/6] Python unit tests =="
.venv/bin/python -m pytest backend/tests/ tests/ -q

echo "== [3/6] YAML config validation =="
for f in configs/*.yaml configs/reproductions/*.yaml; do
  ./target/debug/agent-workbench validate --config "$f" >/dev/null
done
echo "  $(ls configs/*.yaml configs/reproductions/*.yaml | wc -l) configs OK"

echo "== [4/6] API integration smoke =="
.venv/bin/python - <<'PY'
from fastapi.testclient import TestClient
from backend.main import app
c = TestClient(app)
assert c.get("/health").json()["status"] == "healthy"
r = c.post("/api/v1/config/validate", content="agent:\n  name: T\n  model: qwen3.6-plus\n",
           headers={"Content-Type": "text/plain"})
assert r.json()["valid"] is True
print("  API OK")
PY

echo "== [5/6] Benchmark smoke (all studies) =="
for study in react plan_and_solve reflexion swebench gaia taubench; do
  .venv/bin/python -m benchmarks.run --study "$study" --limit 1 >/dev/null
  echo "  $study OK"
done

echo "== [6/6] Frontend lint + build =="
(cd frontend && npm run lint && npm run build)

if [[ -f .env ]]; then
  echo "== [optional] Live LLM exec =="
  set -a && source .env && set +a
  out=$(timeout 30 ./target/debug/agent-workbench exec \
    --config configs/qwen_agent.yaml --prompt "Reply OK" --no-tools 2>&1 || true)
  echo "$out" | rg -q '"type":"run_complete"' && echo "  Qwen exec OK" || echo "  Qwen exec skipped/failed"
fi

echo ""
echo "All tests passed."
