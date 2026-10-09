#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
# Separate feasibility oracle: no fetched Go modules or application/build dependency.
GOTOOLCHAIN=local GOPROXY=off GOSUMDB=off go run ./spikes/templates/main.go < ./spikes/templates/tests/cases.json
