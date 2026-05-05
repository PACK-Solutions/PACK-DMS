#!/usr/bin/env bash
# Create a new SQLx migration file with a UTC timestamp prefix.
#
# Usage: new_migration.sh <short_description>
#   e.g. new_migration.sh add_widget_tier_column
#
# Picks a timestamp strictly greater than any existing migration so the
# new file is always applied last, even if the wall clock has skewed.

set -euo pipefail

if [[ $# -ne 1 ]]; then
    echo "usage: $(basename "$0") <short_description>" >&2
    exit 1
fi

description="$1"

# Sanity-check the description: snake_case, no spaces, no leading digit.
if ! [[ "$description" =~ ^[a-z][a-z0-9_]*$ ]]; then
    echo "error: description must be lowercase snake_case (got: $description)" >&2
    exit 1
fi

# Locate the migrations directory relative to the repo root.
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../../../.." && pwd)"
migrations_dir="$repo_root/migrations"

if [[ ! -d "$migrations_dir" ]]; then
    echo "error: $migrations_dir does not exist" >&2
    exit 1
fi

now="$(date -u +%Y%m%d%H%M%S)"

# If the most recent migration's prefix is >= now (clock skew, rapid creation
# in the same second, etc.), bump to one second past it.
latest_prefix="$(ls "$migrations_dir" 2>/dev/null \
    | grep -E '^[0-9]+_' \
    | sed -E 's/^([0-9]+)_.*$/\1/' \
    | sort -n \
    | tail -n 1 || true)"

if [[ -n "${latest_prefix:-}" ]] && [[ "$latest_prefix" =~ ^[0-9]{14}$ ]] && [[ "$latest_prefix" -ge "$now" ]]; then
    # Add one second using date arithmetic.
    if date -u -j -v+1S -f "%Y%m%d%H%M%S" "$latest_prefix" "+%Y%m%d%H%M%S" >/dev/null 2>&1; then
        # BSD date (macOS)
        now="$(date -u -j -v+1S -f "%Y%m%d%H%M%S" "$latest_prefix" "+%Y%m%d%H%M%S")"
    else
        # GNU date (Linux)
        formatted="${latest_prefix:0:4}-${latest_prefix:4:2}-${latest_prefix:6:2} ${latest_prefix:8:2}:${latest_prefix:10:2}:${latest_prefix:12:2}"
        now="$(date -u -d "$formatted UTC + 1 second" +%Y%m%d%H%M%S)"
    fi
fi

target="$migrations_dir/${now}_${description}.sql"

if [[ -e "$target" ]]; then
    echo "error: $target already exists" >&2
    exit 1
fi

cat > "$target" <<EOF
-- Migration: ${description}
-- Created: $(date -u +"%Y-%m-%d %H:%M:%S UTC")
--
-- TODO: describe the change and the reason here.

-- Add idempotent DDL below. Examples:
--   CREATE TABLE IF NOT EXISTS ...
--   ALTER TABLE x ADD COLUMN IF NOT EXISTS y ...
--   CREATE INDEX IF NOT EXISTS ix_... ON ...
EOF

echo "$target"
