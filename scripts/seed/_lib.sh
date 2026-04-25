#!/usr/bin/env bash
# Shared helpers for the seed scripts under `scripts/seed/`.
#
# Sourced (not executed) by every seed script. Provides a single
# place to:
#
#   - locate the `progest` binary (debug build under target/, or
#     whatever is on $PATH if the user opted in via PROGEST_BIN);
#   - print colorized status lines;
#   - tear down + recreate the seed output directory;
#   - run `progest init` and `progest scan` consistently.
#
# Conventions:
#   - Every seed script accepts a single positional argument: the
#     output directory to write the project into. Defaults to
#     `./tmp/seed/<pattern>/` so multiple patterns coexist.
#   - Scripts are idempotent: running again wipes the previous
#     output before recreating. This is intentional — these are
#     throwaway test fixtures, not user data.

set -euo pipefail

# Resolve the absolute path of the repository root from this file's
# location, regardless of where the caller cd'd to.
SEED_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SEED_LIB_DIR}/../.." && pwd)"

# `progest` resolution order:
#   1. $PROGEST_BIN if set (lets users point at a release build).
#   2. `cargo run -p progest-cli --` if neither of the above exists.
#   3. `target/debug/progest` if it has been built.
progest() {
    if [[ -n "${PROGEST_BIN:-}" ]]; then
        "${PROGEST_BIN}" "$@"
        return
    fi
    local debug_bin="${REPO_ROOT}/target/debug/progest"
    if [[ -x "${debug_bin}" ]]; then
        "${debug_bin}" "$@"
        return
    fi
    # Fall back to `cargo run`. Slow on first invocation but always
    # works without a separate build step.
    (cd "${REPO_ROOT}" && cargo run --quiet -p progest-cli -- "$@")
}

# Color helpers — only emit codes when stdout is a TTY.
if [[ -t 1 ]]; then
    BOLD=$'\033[1m'; CYAN=$'\033[36m'; YELLOW=$'\033[33m'; RESET=$'\033[0m'
else
    BOLD=''; CYAN=''; YELLOW=''; RESET=''
fi

step() {
    printf '%s==>%s %s%s%s\n' "${CYAN}" "${RESET}" "${BOLD}" "$*" "${RESET}"
}

note() {
    printf '%s   %s%s\n' "${YELLOW}" "$*" "${RESET}"
}

# Default output dir for a pattern. Caller can override with $1.
default_out_dir() {
    local pattern="$1"
    printf '%s/tmp/seed/%s' "${REPO_ROOT}" "${pattern}"
}

# Tear down an existing seed output and recreate it fresh.
prepare_dir() {
    local out="$1"
    if [[ -d "${out}" ]]; then
        step "Removing existing seed output at ${out}"
        rm -rf "${out}"
    fi
    mkdir -p "${out}"
}

# Run `progest init` against `out` with the supplied project name.
seed_init() {
    local out="$1"
    local name="$2"
    step "progest init --name ${name}"
    (cd "${out}" && progest init --name "${name}" >/dev/null)
}

# Run `progest scan` against `out`.
seed_scan() {
    local out="$1"
    step "progest scan"
    (cd "${out}" && progest scan >/dev/null)
}

# Convenience: write a small file with the given (project-relative)
# path, creating any intermediate directories. The body is read from
# stdin so heredocs compose cleanly.
seed_write() {
    local out="$1"
    local rel="$2"
    local target="${out}/${rel}"
    mkdir -p "$(dirname "${target}")"
    cat > "${target}"
}

# Same as seed_write but for an empty file (touches a placeholder).
seed_touch() {
    local out="$1"
    local rel="$2"
    local target="${out}/${rel}"
    mkdir -p "$(dirname "${target}")"
    : > "${target}"
}

# Print a final summary line so users know the project is ready.
seed_done() {
    local out="$1"
    local pattern="$2"
    printf '\n'
    printf '%s%s seed pattern ready:%s %s\n' "${BOLD}" "${pattern}" "${RESET}" "${out}"
    printf '   cd %s\n' "${out}"
    printf '   progest search "is:violation" --format json | jq .\n'
}
