#!/usr/bin/env bash
set -euo pipefail

readonly script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly repo_root="$(cd -- "$script_dir/.." && pwd)"
# shellcheck source=aur_checks.sh
source "$script_dir/aur_checks.sh"
readonly default_aur_dir="$(cd -- "$repo_root/.." && pwd)/amdtop-aur"
aur_dir="${AUR_DIR:-$default_aur_dir}"

usage() {
  cat <<'EOF'
Usage: scripts/update-aur.sh [--publish] [VERSION]

Prepare and validate the sibling amdtop-aur checkout. Without --publish, an
interactive terminal must provide the exact confirmation before pushing; a
non-interactive run stops after preparation. --publish is the explicit
confirmation for an authorized local deploying agent and remains disabled in
CI.

VERSION defaults to the version in Cargo.toml. Intentional edits to PKGBUILD
(and a regenerated .SRCINFO) are preserved; increment pkgrel for packaging-only
changes. Set AUR_DIR to override the sibling checkout path.
EOF
}

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

publish=false
version=''
while (( $# > 0 )); do
  case "$1" in
    -h | --help)
      usage
      exit 0
      ;;
    --publish)
      publish=true
      ;;
    -*)
      fail "unknown option: $1"
      ;;
    *)
      [[ -z "$version" ]] || fail 'only one VERSION may be specified'
      version="$1"
      ;;
  esac
  shift
done

note() {
  printf '==> %s\n' "$*"
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

if [[ -n "${CI:-}" ]]; then
  fail 'this maintainer publishing tool is intentionally disabled in CI'
fi

for command in awk b2sum bsdtar cargo curl git grep jq makepkg sed sleep sort tar vercmp; do
  require_command "$command"
done

if [[ -z "$version" ]]; then
  version="$(
    cargo metadata \
      --manifest-path "$repo_root/Cargo.toml" \
      --locked \
      --no-deps \
      --format-version 1 |
      jq -er '.packages[] | select(.name == "amdtop") | .version'
  )"
fi

[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.][0-9A-Za-z]+)*$ ]] ||
  fail "invalid release version: $version"

current_version="$(
  cargo metadata \
    --manifest-path "$repo_root/Cargo.toml" \
    --locked \
    --no-deps \
    --format-version 1 |
    jq -er '.packages[] | select(.name == "amdtop") | .version'
)"
[[ "$version" == "$current_version" ]] ||
  fail "requested version $version does not match Cargo.toml version $current_version"

[[ -d "$aur_dir/.git" ]] ||
  fail "AUR checkout not found at $aur_dir (override with AUR_DIR)"
[[ "$(git -C "$aur_dir" branch --show-current)" == master ]] ||
  fail 'the AUR checkout must be on its master branch'

changed_paths="$(
  {
    git -C "$aur_dir" diff --name-only
    git -C "$aur_dir" diff --cached --name-only
    git -C "$aur_dir" ls-files --others --exclude-standard
  } | sort -u
)"
unexpected_paths="$(printf '%s\n' "$changed_paths" | grep -Ev '^(|PKGBUILD|\.SRCINFO)$' || true)"
[[ -z "$unexpected_paths" ]] ||
  fail "the AUR checkout has unexpected changes: $unexpected_paths"

readonly expected_aur_url='ssh://aur@aur.archlinux.org/amdtop.git'
readonly expected_github_url='git@github.com:lhl/amdtop-aur.git'
[[ "$(git -C "$aur_dir" remote get-url aur)" == "$expected_aur_url" ]] ||
  fail "the aur remote must be $expected_aur_url"
[[ "$(git -C "$aur_dir" remote get-url github)" == "$expected_github_url" ]] ||
  fail "the github remote must be $expected_github_url"

if [[ -z "$changed_paths" ]]; then
  note 'Synchronizing the GitHub packaging mirror'
  git -C "$aur_dir" pull --ff-only github master
else
  note 'Checking the GitHub mirror before preserving local packaging edits'
  git -C "$aur_dir" fetch github master
  [[ "$(git -C "$aur_dir" rev-parse HEAD)" == "$(git -C "$aur_dir" rev-parse github/master)" ]] ||
    fail 'the dirty AUR checkout is not synchronized with the GitHub mirror'
fi

local_head="$(git -C "$aur_dir" rev-parse HEAD)"
aur_head="$(git -C "$aur_dir" ls-remote aur refs/heads/master | awk '{print $1}')"
if [[ -n "$aur_head" && "$aur_head" != "$local_head" ]]; then
  fail 'the AUR and GitHub packaging histories differ; reconcile them manually'
fi

packaged_version="$(sed -n 's/^pkgver=//p' "$aur_dir/PKGBUILD")"
packaged_release="$(sed -n 's/^pkgrel=//p' "$aur_dir/PKGBUILD")"
[[ "$packaged_release" =~ ^[1-9][0-9]*$ ]] ||
  fail "invalid pkgrel in the AUR PKGBUILD: $packaged_release"
if [[ "$version" != "$packaged_version" ]]; then
  packaged_release=1
fi
expected_version="$version-$packaged_release"
indexed_version="$(
  curl --fail --silent --show-error \
    'https://aur.archlinux.org/rpc/v5/info?arg[]=amdtop' |
    jq -r '.results[0].Version // empty'
)"
if [[ "$indexed_version" == "$expected_version" ]]; then
  if [[ -z "$changed_paths" ]]; then
    note "AUR and GitHub already publish amdtop $expected_version; nothing to do"
    exit 0
  fi
  fail "AUR already publishes $expected_version; increment pkgrel for packaging changes"
fi
if [[ -n "$indexed_version" ]] &&
  (( $(vercmp "$expected_version" "$indexed_version") < 0 )); then
  fail "refusing to replace AUR version $indexed_version with older $expected_version"
fi

archive_url="https://static.crates.io/crates/amdtop/amdtop-$version.crate"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
archive="$tmp_dir/amdtop-$version.crate"

note "Downloading the published amdtop $version crate"
curl --fail --location --silent --show-error "$archive_url" --output "$archive"
checksum="$(b2sum "$archive" | awk '{print $1}')"

archive_listing="$tmp_dir/archive-listing"
tar -tf "$archive" >"$archive_listing"
aur_check_archive_listing "$archive_listing" "$version" ||
  fail 'published crate is missing required package or notice files'

note 'Updating PKGBUILD metadata and source checksum'
sed -Ei \
  -e "s/^pkgver=.*/pkgver=$version/" \
  -e "s/^pkgrel=.*/pkgrel=$packaged_release/" \
  -e "s/^b2sums=.*/b2sums=('$checksum')/" \
  "$aur_dir/PKGBUILD"

makepkg --dir "$aur_dir" --printsrcinfo >"$aur_dir/.SRCINFO"
aur_check_srcinfo "$aur_dir/.SRCINFO" ||
  fail 'update PKGBUILD licensing before publishing the AUR package'
makepkg --dir "$aur_dir" --verifysource --force

if command -v pkgctl >/dev/null 2>&1; then
  note 'Checking AUR package-source licensing'
  pkgctl license check "$aur_dir"
fi

if command -v pkgctl >/dev/null 2>&1; then
  note 'Building in a reusable clean Arch chroot'
  pkgctl build "$aur_dir"
else
  note 'pkgctl is unavailable; using a clean local makepkg build'
  makepkg --dir "$aur_dir" --cleanbuild --clean --force
fi

pkgfile="$(makepkg --dir "$aur_dir" --packagelist | head -n 1)"
[[ -f "$pkgfile" ]] || fail "built package not found: $pkgfile"

if command -v namcap >/dev/null 2>&1; then
  note 'Running namcap'
  PATH="/usr/bin:$PATH" namcap "$aur_dir/PKGBUILD" "$pkgfile"
else
  printf 'warning: namcap is unavailable; package lint was skipped\n' >&2
fi

package_root="$tmp_dir/package-root"
mkdir -p "$package_root"
bsdtar -xf "$pkgfile" -C "$package_root"
aur_check_package_notices "$package_root" ||
  fail 'update PKGBUILD to install all upstream notices'
reported_version="$("$package_root/usr/bin/amdtop" --version)"
[[ "$reported_version" == "amdtop $version" ]] ||
  fail "packaged binary reported '$reported_version', expected 'amdtop $version'"

note 'Reviewing the proposed AUR update'
git -C "$aur_dir" diff HEAD --check
git -C "$aur_dir" status -sb
git -C "$aur_dir" diff HEAD -- PKGBUILD .SRCINFO LICENSE REUSE.toml
note 'Reviewing the packaged file list'
bsdtar -tf "$pkgfile"
printf '\nPackage: %s\nVersion: %s-%s\nSource BLAKE2b: %s\n' \
  "$pkgfile" "$version" "$packaged_release" "$checksum"

confirmation="publish amdtop $version-$packaged_release"
if [[ "$publish" == true ]]; then
  note "Explicit --publish confirmation accepted for amdtop $version-$packaged_release"
else
  if [[ ! -t 0 || ! -t 1 ]]; then
    note 'Preparation complete; refusing to publish without --publish or an interactive terminal'
    exit 0
  fi

  printf '\nType %q to commit and push to the AUR and GitHub mirror: ' "$confirmation"
  read -r answer
  if [[ "$answer" != "$confirmation" ]]; then
    note 'Publication cancelled; prepared changes remain in the AUR checkout'
    exit 0
  fi
fi

if ! git -C "$aur_dir" diff HEAD --quiet -- PKGBUILD .SRCINFO; then
  git -C "$aur_dir" add PKGBUILD .SRCINFO
  git -C "$aur_dir" diff --cached --check
  git -C "$aur_dir" diff --cached
  if [[ "$packaged_release" == 1 ]]; then
    commit_message="Update to $version"
  else
    commit_message="Revise package release $version-$packaged_release"
  fi
  git -C "$aur_dir" commit -m "$commit_message"
else
  note 'No packaging commit is needed; publishing the current reviewed commit'
fi

note 'Publishing to the AUR'
git -C "$aur_dir" push aur HEAD:master
note 'Updating the GitHub packaging mirror'
git -C "$aur_dir" push github HEAD:master

note "Waiting for the AUR to index amdtop $expected_version"
aur_index_attempts=${AUR_INDEX_ATTEMPTS:-120}
aur_index_interval=${AUR_INDEX_INTERVAL:-5}
if ! aur_wait_for_index \
  "$expected_version" "$aur_index_attempts" "$aur_index_interval"; then
  fail "AUR pushes completed, but the public index still reports '${AUR_INDEXED_VERSION:-unavailable}' instead of '$expected_version'; verify the AUR page before retrying publication"
fi

note "Published and verified amdtop $expected_version"
