#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APPS_DIR="$ROOT_DIR/eon-apps"

if ! git -C "$ROOT_DIR" rev-parse --show-toplevel >/dev/null 2>&1; then
    printf 'Error: %s is not inside a Git repository.\n' "$ROOT_DIR" >&2
    exit 1
fi

if ! git -C "$APPS_DIR" rev-parse --show-toplevel >/dev/null 2>&1; then
    printf 'Error: eon-apps repository not found at %s\n' "$APPS_DIR" >&2
    exit 1
fi

read -r -p 'EonOS commit message: ' EONOS_MESSAGE
if [[ -z "${EONOS_MESSAGE//[[:space:]]/}" ]]; then
    printf 'Error: EonOS commit message cannot be empty.\n' >&2
    exit 1
fi

read -r -p 'eon-apps commit message: ' APPS_MESSAGE
if [[ -z "${APPS_MESSAGE//[[:space:]]/}" ]]; then
    printf 'Error: eon-apps commit message cannot be empty.\n' >&2
    exit 1
fi

push_repo() {
    local repo_dir="$1"
    local repo_name="$2"
    local message="$3"
    local branch

    if ! git -C "$repo_dir" remote get-url origin >/dev/null 2>&1; then
        printf 'Error: %s has no origin remote configured.\n' "$repo_name" >&2
        return 1
    fi

    branch="$(git -C "$repo_dir" branch --show-current)"
    if [[ -z "$branch" ]]; then
        printf 'Error: %s is in detached HEAD state; check out a branch first.\n' "$repo_name" >&2
        return 1
    fi

    printf '\n==> Preparing %s (%s)\n' "$repo_name" "$branch"
    git -C "$repo_dir" add -A

    if git -C "$repo_dir" diff --cached --quiet; then
        printf 'No changes to commit in %s.\n' "$repo_name"
    else
        git -C "$repo_dir" commit -m "$message"
    fi

    git -C "$repo_dir" push -u origin "$branch"
}

push_repo "$ROOT_DIR" 'EonOS' "$EONOS_MESSAGE"
push_repo "$APPS_DIR" 'eon-apps' "$APPS_MESSAGE"

printf '\nBoth repositories are up to date with origin.\n'
