# OpenGEO SDK codegen — Story 12.3.
#
# Pinned generator versions (bumps go through a deliberate PR with the
# regenerated SDK diffs).

ORVAL_VERSION             := 7.1.0
OPENAPI_PYTHON_VERSION    := 0.24.0

SPEC := $(realpath ../../crates/wire-schema/openapi.json)
TS_DIR := $(realpath ../../packages/typescript)
PY_DIR := $(realpath ../../packages/python)

.PHONY: sdks ts py drift help

help:
	@echo "Targets:"
	@echo "  sdks    Regenerate both TypeScript + Python SDKs from the canonical spec."
	@echo "  ts      Regenerate TypeScript only."
	@echo "  py      Regenerate Python only."
	@echo "  drift   CI gate: regenerate to tmp + diff -q against committed artifacts."

sdks: ts py

ts:
	@if ! command -v npx >/dev/null 2>&1; then \
		echo "warning: npx not available; install Node.js to regenerate the TS SDK"; \
		exit 1; \
	fi
	@echo "Regenerating TypeScript SDK with orval@$(ORVAL_VERSION)…"
	cd .. && npx -y orval@$(ORVAL_VERSION) --config infra/codegen/orval.config.cjs

py:
	@if ! command -v openapi-python-client >/dev/null 2>&1; then \
		echo "warning: openapi-python-client not available; install with: pipx install openapi-python-client==$(OPENAPI_PYTHON_VERSION)"; \
		exit 1; \
	fi
	@echo "Regenerating Python SDK with openapi-python-client@$(OPENAPI_PYTHON_VERSION)…"
	cd .. && openapi-python-client generate --path $(SPEC) --config infra/codegen/openapi-python.yaml --meta none --output-path $(PY_DIR) --overwrite

drift:
	@bash tests/drift.sh
