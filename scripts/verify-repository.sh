#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

required=(
  README.md
  SECURITY.md
  assets/banner.png
  docs/architecture.md
  docs/auction-and-routing.md
  docs/capital-and-risk.md
  docs/economic-model.md
  docs/integration.md
  docs/operations.md
  docs/settlement-lifecycle.md
  sdk/client.js
  src/capital.rs
)

for artifact in "${required[@]}"; do
  test -f "$artifact" || { echo "missing artifact: $artifact" >&2; exit 1; }
done

document_count="$(find docs -maxdepth 1 -type f -name '*.md' | wc -l | tr -d ' ')"
test "$document_count" = "7" || { echo "expected 7 documents, found $document_count" >&2; exit 1; }

banner_bytes="$(wc -c < assets/banner.png | tr -d ' ')"
test "$banner_bytes" -ge 100000 || { echo "banner is below the minimum size" >&2; exit 1; }

diagram_count="$(grep -Rho '```mermaid' README.md SECURITY.md docs | wc -l | tr -d ' ')"
test "$diagram_count" -ge 26 || { echo "expected at least 26 diagrams, found $diagram_count" >&2; exit 1; }

rust_lines="$(find src -type f -name '*.rs' -exec awk 'NF { count++ } END { print count + 0 }' {} +)"
test "$rust_lines" -ge 5000 || { echo "Rust surface is unexpectedly small" >&2; exit 1; }

javascript_lines="$(find sdk tests/node -type f -name '*.js' -exec awk 'NF { count++ } END { print count + 0 }' {} +)"
test "$javascript_lines" -ge 250 || { echo "JavaScript surface is unexpectedly small" >&2; exit 1; }

grep -q '^version = "1.0.0"' Cargo.toml
grep -q '"version": "1.0.0"' package.json
git -c core.autocrlf=true diff --check

echo "repository artifacts ok: $document_count documents, $diagram_count diagrams, $rust_lines Rust lines, $javascript_lines JavaScript lines"
