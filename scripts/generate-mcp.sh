#!/bin/sh
# Regenerate the checked-in tools, OpenAPI and coverage artifacts from the HTTP route registry.
set -eu
cd "$(dirname "$0")/.."
HBOTS_GENERATE_MCP=1 cargo test -q --bin hbots api::management::tests::generated_catalog_is_current
