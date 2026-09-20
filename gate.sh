#!/usr/bin/env bash
# The one verification gate: format, lint, test, in that order, failing on the
# first. CI runs the same things as separate Nix checks; this is the single
# invocation for local and agent use.
#
# `lazyspec validate` is deliberately not here. The docs tree carries
# pre-existing findings from before the rules that catch them existed, so the
# gate would be red on arrival and tell you nothing about your change.
#
# `gate.sh fast` drops the clippy pass, since it's most of the wall clock. Use
# it per-unit mid-batch; run the full gate before the batch is called done.
set -euo pipefail

cargo fmt --all -- --check

if [[ ${1:-} != fast ]]; then
	cargo clippy --all-targets --offline -- -D warnings
fi

cargo test --offline
