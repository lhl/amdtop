# Shared, deterministic validation for the AUR release helper.

# Verify that the published crates.io source carries every file needed by the
# Arch package and its license/attribution installation.
aur_check_archive_listing() {
  local listing=$1
  local version=$2
  local file

  for file in Cargo.lock LICENSE NOTICE THIRD_PARTY.md; do
    if ! grep -Fxq "amdtop-$version/$file" "$listing"; then
      printf 'published crate does not contain the expected %s\n' "$file" >&2
      return 1
    fi
  done
}

# Keep AUR metadata synchronized with amdtop's upstream license.
aur_check_srcinfo() {
  local srcinfo=$1

  if ! grep -Eq '^[[:space:]]+license = Apache-2\.0$' "$srcinfo"; then
    printf 'AUR .SRCINFO does not declare license = Apache-2.0\n' >&2
    return 1
  fi
}

aur_now_seconds() {
  printf '%s\n' "$SECONDS"
}

# Wait for aurweb's public package index after the Git push has succeeded.
# AUR_INDEXED_VERSION is retained for a precise timeout diagnostic. Both the
# overall polling window and every network request are bounded.
aur_wait_for_index() {
  local expected_version=$1
  local timeout=${2:-600}
  local interval=${3:-5}
  local request_timeout=${4:-10}
  local endpoint=${5:-'https://aur.archlinux.org/rpc/v5/info?arg[]=amdtop'}
  local deadline now remaining curl_timeout indexed_version=''

  [[ "$timeout" =~ ^[1-9][0-9]*$ ]] || {
    printf 'AUR index timeout must be a positive integer\n' >&2
    return 1
  }
  [[ "$interval" =~ ^[1-9][0-9]*$ ]] || {
    printf 'AUR index interval must be a positive integer\n' >&2
    return 1
  }
  [[ "$request_timeout" =~ ^[1-9][0-9]*$ ]] || {
    printf 'AUR index request timeout must be a positive integer\n' >&2
    return 1
  }

  AUR_INDEXED_VERSION=''
  deadline=$(($(aur_now_seconds) + timeout))
  while :; do
    now=$(aur_now_seconds)
    ((now < deadline)) || return 1
    remaining=$((deadline - now))
    curl_timeout=$request_timeout
    ((curl_timeout <= remaining)) || curl_timeout=$remaining

    if indexed_version="$(
      curl --fail --silent --show-error \
        --connect-timeout "$curl_timeout" \
        --max-time "$curl_timeout" \
        --header 'Cache-Control: no-cache' \
        "$endpoint" |
        jq -er '.results[0].Version'
    )"; then
      AUR_INDEXED_VERSION=$indexed_version
      [[ "$indexed_version" == "$expected_version" ]] && return 0
    fi

    now=$(aur_now_seconds)
    remaining=$((deadline - now))
    ((remaining > interval)) || return 1
    sleep "$interval"
  done
}

# Verify that the built binary package redistributes upstream legal notices.
aur_check_package_notices() {
  local package_root=$1
  local license_dir="$package_root/usr/share/licenses/amdtop"
  local file

  for file in LICENSE NOTICE THIRD_PARTY.md; do
    if [[ ! -f "$license_dir/$file" ]]; then
      printf 'built package does not contain usr/share/licenses/amdtop/%s\n' "$file" >&2
      return 1
    fi
  done
}
