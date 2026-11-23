#!/usr/bin/env bash
set -euo pipefail

name=${1:?Usage: create-snapshot.sh SNAPSHOT_NAME [GIT_REF]}
ref=${2:-HEAD}
[[ "$name" =~ ^horizon-ci-v[1-9][0-9]*$ ]] || { echo "Use a new horizon-ci-vN name" >&2; exit 1; }

cd "$(git rev-parse --show-toplevel)"
tree=$(git rev-parse --verify "$ref^{tree}")
tmp=$(mktemp -d)
sandbox=""
trap '
  rm -rf "$tmp"
  if [[ -n "$sandbox" ]]; then
    boat delete "$sandbox" --yes || echo "Could not delete $sandbox; it has a 30-minute TTL" >&2
  fi
' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

existing=$(boat snapshots --json | jq -r --arg name "$name" '.named[] | select(.name == $name) | .name')
if [[ -n "$existing" ]]; then
  echo "Refusing to replace $name; use a new version" >&2
  exit 1
fi

git archive --format=tar.gz -o "$tmp/bootstrap.tgz" "$tree" \
  Cargo.lock web/package.json web/pnpm-lock.yaml tools/ci/boat/provision.sh tools/web/wasm-bindgen-dir.sh
echo "Provisioning $name from Git tree $tree"
status=0
boat new --name "$name-builder" --no-env --ttl 1800 --json > "$tmp/new.jsonl" || status=$?
sandbox=$(jq -sr 'map(select(.id? | strings | startswith("bx_"))) | first | .id // empty' "$tmp/new.jsonl")
if (( status != 0 )) || [[ -z "$sandbox" ]] || ! jq -se 'any(.[]; .event == "ready")' "$tmp/new.jsonl" >/dev/null; then
  cat "$tmp/new.jsonl" >&2
  exit 1
fi

boat scp "$tmp/bootstrap.tgz" "$sandbox:/tmp/horizon-ci-bootstrap.tgz"
provision=$(cat <<'BASH'
set -euo pipefail
bootstrap=$(mktemp -d)
trap 'rm -rf "$bootstrap" /tmp/horizon-ci-bootstrap.tgz' EXIT
tar -xzf /tmp/horizon-ci-bootstrap.tgz -C "$bootstrap"
cd "$bootstrap"
bash tools/ci/boat/provision.sh
BASH
)
boat exec "$sandbox" --timeout 900 --stream -- bash -c "$provision"
boat snapshot "$sandbox" "$name"

for (( attempt=0; attempt<60; attempt++ )); do
  status=$(boat snapshots --json | jq -r --arg name "$name" '.named[] | select(.name == $name) | .status')
  case "$status" in
    ready) echo "$name is ready"; exit 0 ;;
    ''|saving) sleep 5 ;;
    *) echo "$name has snapshot status: $status" >&2; exit 1 ;;
  esac
done
echo "Timed out waiting for $name to become ready" >&2
exit 1
