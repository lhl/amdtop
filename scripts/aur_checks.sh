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
