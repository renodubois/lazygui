#!/usr/bin/env bash
# Build-first client restart loop. Run from any directory; see --help.
set -euo pipefail

if [[ ${1:-} == --help || ${1:-} == -h ]]; then
    printf '%s\n' \
        'Usage: dev.sh' \
        'Requires Bash, Rust/Cargo, Watchexec, and setsid (util-linux).' \
        'Install Watchexec: cargo install watchexec-cli --locked' \
        'Builds on startup and source changes; restarts only after success.' \
        'Uses .env.dev-config and target; Ctrl+C stops owned processes.' \
        'Default data is in-memory; network is explicit opt-in. No keyring is used.'
    exit 0
fi
if (( $# != 0 )); then
    printf 'Unexpected arguments. Use --help for usage.\n' >&2
    exit 2
fi

for command in cargo rustc watchexec setsid; do
    if ! command -v "$command" >/dev/null 2>&1; then
        printf 'Missing dependency: %s. See dev.sh --help.\n' "$command" >&2
        exit 1
    fi
done

client_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
cd -- "$client_dir"
# Cargo parses TOML; its read-manifest output starts with the root package name.
manifest=$(cargo read-manifest)
if [[ $manifest =~ ^\{\"name\":\"([a-zA-Z0-9_-]+)\" ]]; then
    package=${BASH_REMATCH[1]}
else
    printf 'Could not read the Cargo package name.\n' >&2
    exit 1
fi
# Always use this worktree's profile, even with an inherited XDG_CONFIG_HOME.
export XDG_CONFIG_HOME="$client_dir/.env.dev-config"
# Explicitly build for this machine, not an inherited cross-compilation target.
host_target=$(rustc --print host-tuple)

state_dir=$(mktemp -d)
watcher_pid=
build_pid=
client_pid=

# Each child has a private process group. Never stop another development session.
stop_process() {
    local pid=${1:-}
    [[ -n $pid ]] || return 0
    kill -TERM -- "-$pid" 2>/dev/null || true
    for (( attempt = 0; attempt < 30; attempt++ )); do
        kill -0 -- "-$pid" 2>/dev/null || break
        sleep 0.1
    done
    kill -KILL -- "-$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
}

cleanup() {
    trap '' INT TERM HUP
    stop_process "$watcher_pid"
    stop_process "$build_pid"
    stop_process "$client_pid"
    rm -rf -- "$state_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

# The watcher only queues requests. The supervisor owns builds and the client,
# so a new save never kills the running client or interrupts compilation.
mkfifo "$state_dir/changes"
exec 3<>"$state_dir/changes"
setsid watchexec \
    --debounce 300ms \
    --on-busy-update queue \
    --watch src \
    --watch Cargo.toml \
    --watch Cargo.lock \
    --shell=none \
    -- bash -c 'printf "rebuild\n" > "$1"' bash "$state_dir/changes" \
    3>&- </dev/null &
watcher_pid=$!

printf 'Watching project sources. Ctrl+C to stop.\n'
while true; do
    # A timeout lets us notice a failed watcher instead of waiting forever.
    if ! IFS= read -r -t 1 request <&3; then
        if ! kill -0 "$watcher_pid" 2>/dev/null; then
            printf 'File watcher stopped unexpectedly.\n' >&2
            exit 1
        fi
        continue
    fi
    # Coalesce saves made during the previous build into one fresh build.
    while IFS= read -r -t 0.01 request <&3; do :; done

    printf 'Building client...\n'
    setsid cargo build --locked --bin "$package" --target "$host_target" \
        --target-dir "$client_dir/target" \
        3>&- </dev/null &
    build_pid=$!
    if wait "$build_pid"; then
        build_pid=
        binary="$client_dir/target/$host_target/debug/$package"
        if [[ ! -x $binary ]]; then
            printf 'Expected native debug binary at %s; check Cargo target configuration.\n' "$binary" >&2
            continue
        fi
        stop_process "$client_pid"
        printf 'Starting rebuilt client...\n'
        setsid "$binary" 3>&- </dev/null &
        client_pid=$!
    else
        build_pid=
        printf 'Build failed; keeping the current client. Save a fix to retry.\n' >&2
    fi
done
