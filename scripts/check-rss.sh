#!/usr/bin/env bash
set -euo pipefail

if (( $# < 2 || $# > 4 )); then
    printf 'Usage: %s PID MAX_GROWTH_KIB [DURATION_SECONDS [INTERVAL_SECONDS]]\n' "$0" >&2
    exit 2
fi

pid=$1
limit=$2
duration=${3:-3600}
interval=${4:-60}
for number in "$pid" "$limit" "$duration" "$interval"; do
    if [[ ! $number =~ ^(0|[1-9][0-9]{0,9})$ ]]; then
        printf 'Arguments must be nonnegative integers of up to 10 digits.\n' >&2
        exit 2
    fi
done
if (( pid == 0 || interval == 0 )); then
    printf 'PID and interval must be greater than zero.\n' >&2
    exit 2
fi

start_id() {
    local stat
    IFS= read -r stat 2>/dev/null < "/proc/$pid/stat" || return 1
    # Parse the fields after the command name, which may include spaces or parentheses.
    local -a fields
    read -r -a fields <<< "${stat##*) }"
    printf '%s\n' "${fields[19]:-}"
}

read_rss() {
    local label value unit
    while read -r label value unit; do
        if [[ $label == VmRSS: && $unit == kB ]]; then
            printf '%s\n' "$value"
            return 0
        fi
    done 2>/dev/null < "/proc/$pid/status"
    return 1
}

identity=$(start_id) || identity=
if [[ -z $identity ]]; then
    printf 'Could not read process %s.\n' "$pid" >&2
    exit 2
fi

SECONDS=0
baseline=
peak_growth=0
printf 'elapsed_seconds,rss_kib,growth_kib\n'
while :; do
    if ! rss=$(read_rss) || [[ $(start_id) != "$identity" ]]; then
        printf 'Process %s exited or became unreadable.\n' "$pid" >&2
        exit 2
    fi
    baseline=${baseline:-$rss}
    growth=$((rss - baseline))
    elapsed=$SECONDS
    printf '%s,%s,%s\n' "$elapsed" "$rss" "$growth"
    if (( growth > peak_growth )); then
        peak_growth=$growth
    fi
    if (( elapsed >= duration )); then
        break
    fi
    remaining=$((duration - elapsed))
    sleep "$((interval < remaining ? interval : remaining))"
done

printf 'Peak growth %s KiB (limit %s KiB)\n' "$peak_growth" "$limit" >&2
if (( peak_growth > limit )); then
    exit 1
fi
