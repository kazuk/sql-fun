#!/bin/bash

# Utilities shared by the agent Git wrapper and controlled worktree helper.

agent_real_git() {
    local agent_bin="$1"
    local entry canonical_entry filtered_path real_git
    local -a entries filtered_entries

    agent_bin=$(cd "$agent_bin" && pwd -P)
    IFS=: read -r -a entries <<<"${PATH:-}"

    for entry in "${entries[@]}"; do
        if [[ -z "$entry" ]]; then
            canonical_entry=$(pwd -P)
        elif [[ -d "$entry" ]]; then
            canonical_entry=$(cd "$entry" && pwd -P)
        else
            canonical_entry=""
        fi

        if [[ "$canonical_entry" != "$agent_bin" ]]; then
            filtered_entries+=("$entry")
        fi
    done

    filtered_path=$(IFS=:; printf '%s' "${filtered_entries[*]}")
    real_git=$(PATH="$filtered_path" command -v git)

    if [[ -z "$real_git" || ! -x "$real_git" ]]; then
        echo "Unable to resolve the real Git executable." >&2
        return 1
    fi

    realpath "$real_git"
}
