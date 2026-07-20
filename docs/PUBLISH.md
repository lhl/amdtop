# Publishing Checklist

Use this checklist to prepare and publish an `amdtop` release. The initial
`amdtop` 0.2.2 release was published manually; subsequent crates.io releases
should use the trusted-publishing workflow in `.github/workflows/publish.yml`.
The [`amdtop` AUR package](https://aur.archlinux.org/packages/amdtop) is
updated separately after the crates.io artifact has been published and
verified. A release is not complete until both registries report the new
version.

## One-Time Trusted Publishing Setup

The workflow uses GitHub Actions OIDC to obtain a temporary crates.io token. No
long-lived crates.io token or GitHub Actions secret is needed.

Complete these steps after `publish.yml` is present on the default branch:

1. Open the repository's [GitHub Actions environments settings](https://github.com/lhl/amdtop/settings/environments)
   and create an environment named `release`.
2. Under the environment's deployment branches and tags, restrict deployments
   to tags matching `v*` if that option is available.
3. Add required reviewers only if another trusted reviewer is available. A sole
   maintainer should not configure a rule that prevents self-review.
4. Open the [`amdtop` settings on crates.io](https://crates.io/crates/amdtop/settings),
   find **Trusted Publishing**, and add a GitHub publisher with these exact
   values:

   | Field | Value |
   |---|---|
   | Repository owner | `lhl` |
   | Repository name | `amdtop` |
   | Workflow filename | `publish.yml` |
   | Environment name | `release` |

5. Revoke the API token used for the initial publication. If it was saved by
   `cargo login`, remove the local copy with `cargo logout` after revoking it on
   crates.io.

The names above are case-sensitive identity constraints. Do not add a
`CARGO_REGISTRY_TOKEN` repository secret. The official
`rust-lang/crates-io-auth-action` exchanges GitHub's signed OIDC identity for a
short-lived token immediately before publication and revokes it when the job
ends.

The trusted publisher grants `publish-update` access to this crate. It does not
grant owner-management or yank permissions.

## One-Time AUR Setup

The AUR packaging lives in the standalone sibling repository
[`lhl/amdtop-aur`](https://github.com/lhl/amdtop-aur). Its `master` branch is
mirrored to the AUR repository; the checkout uses remotes named `github` and
`aur`. Keeping it separate satisfies the AUR Git layout and leaves open the
option of adding it to this repository as a submodule later.

Do not store the AUR private key in GitHub Actions or publish AUR updates from
CI. AUR authentication uses a long-lived SSH key, and a source package still
needs a human review for dependency, license, and build-system changes.

1. On an Arch system, install the packaging tools:

   ```sh
   sudo pacman -S --needed base-devel devtools namcap pacman-contrib
   ```

2. Add the dedicated public key from `~/.ssh/AUR.pub` to the **SSH Public Key**
   field in the [AUR account settings](https://aur.archlinux.org/account/), and
   configure the matching private key:

   ```sshconfig
   Host aur.archlinux.org
     User aur
     IdentityFile ~/.ssh/AUR
     IdentitiesOnly yes
   ```

   Test it with `ssh aur@aur.archlinux.org help`. The AUR provides Git and
   management commands, not an interactive shell.

3. Restore or create the sibling checkout with the expected remote names:

   ```sh
   git clone git@github.com:lhl/amdtop-aur.git ../amdtop-aur
   cd ../amdtop-aur
   git switch master
   git remote add aur ssh://aur@aur.archlinux.org/amdtop.git
   git config user.name lhl
   git config user.email lhl@randomfoo.net
   git remote -v
   ```

   The GitHub mirror and AUR repository must point to the same commit before an
   update. The AUR only accepts pushes to `master`; its first push creates the
   package page.

4. Keep only reviewed AUR source files in this repository: `PKGBUILD`,
   `.SRCINFO`, `.gitignore`, `LICENSE`, `LICENSES/0BSD.txt`, and `REUSE.toml`.
   The recipe builds `amdtop` from the immutable crates.io archive rather than
   publishing a `-bin` or `-git` variant. The packaging files are licensed
   under `0BSD`; `license=('Apache-2.0')` and the installed upstream license
   describe amdtop itself.

5. Use `scripts/update-aur.sh` from the main amdtop checkout for release
   updates. It defaults to `../amdtop-aur`; set `AUR_DIR` only when the sibling
   checkout is elsewhere. The script refuses to run when `CI` is set. It
   requires either an exact interactive confirmation or the explicit
   `--publish` option used by an authorized local deploying agent. It also
   verifies the Apache-2.0 `.SRCINFO` metadata and requires the binary package
   to install `LICENSE`, `NOTICE`, and `THIRD_PARTY.md`.

## Versioning

Follow Semantic Versioning while the project is pre-1.0:

- Patch (`0.2.3`): compatible bug fixes, documentation corrections, and
  maintenance changes.
- Minor (`0.3.0`): meaningful new features or intentionally incompatible
  behavior while the public interface remains pre-1.0.
- Major (`1.0.0`): a stable contract or later breaking changes to that contract.

Published crates.io versions are permanent and cannot be overwritten or
deleted.

## Release Punch List

### Prepare

- [ ] Choose the next version and confirm it does not already exist on
      [crates.io](https://crates.io/crates/amdtop/versions).
- [ ] Synchronize the release base:

  ```sh
  git fetch --tags origin
  git switch main
  git pull --ff-only
  git status -sb
  ```

- [ ] Confirm the tree is clean and `main` matches `origin/main`.
- [ ] Update `version` in `Cargo.toml`.
- [ ] Run `cargo check` once to refresh the root package version in
      `Cargo.lock`, then review the lockfile diff.
- [ ] Move the relevant `CHANGELOG.md` entries from **Unreleased** into a dated
      `## [X.Y.Z] - YYYY-MM-DD` section.
- [ ] Update the changelog comparison links at the bottom of the file.
- [ ] Re-read `README.md`, `CHANGELOG.md`, and this checklist. Update them when
      installation, requirements, telemetry behavior, keybindings, or the
      release process changed.
- [ ] If the TUI layout or displayed telemetry changed, follow the
      [screenshot generation runbook](SCREENSHOT.md), inspect the image and text
      dump, and update `docs/screenshot.png` when it improves the documentation.
- [ ] Review any `libamdgpu_top` version change explicitly; backend updates can
      affect telemetry and GPU power-management behavior without an amdtop API
      change.
- [ ] If native library, compiler, or test requirements changed, note the
      corresponding AUR `depends`, `makedepends`, or `checkdepends` update.
- [ ] Search the proposed package and release diff for credentials, machine
      paths, logs, or other private data.

### Validate

Confirm the active compiler is Rust 1.88 or newer, then run the same gates used
by the publishing workflow. The workflow pins these commands to the minimum
supported Rust 1.88 toolchain.

```sh
rustc --version
cargo fmt --all --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets --all-features
cargo publish --dry-run --locked --allow-dirty
cargo package --locked --allow-dirty --list
```

`--allow-dirty` is intentional for these two local pre-commit checks so they
validate the reviewed release diff. The CI workflow publishes only from a clean
tagged checkout and does not use that option.

- [ ] Confirm all commands pass.
- [ ] Inspect the package file list and ensure it contains only intentional
      source, documentation, metadata, tests, and examples.
- [ ] If GPU or NPU telemetry changed, run the hardware smoke test on an AMD
      system and inspect every detected device:

  ```sh
  cargo run --locked --example smoke
  ```

- [ ] Run `cargo run --locked --bin amdtop` interactively and exercise
      the affected UI paths.
- [ ] Run `git diff --check` and review the complete release diff.

### Commit and Tag

Stage release files explicitly rather than using `git add .`, `git add -A`, or
`git commit -a`:

```sh
git add Cargo.toml Cargo.lock CHANGELOG.md README.md <other-intended-files>
git diff --staged --name-only
git diff --staged
git commit -m "Prepare amdtop X.Y.Z"
git push origin main
```

After the pushed commit and all validation results have been reviewed, create
and push an annotated tag:

```sh
git tag -a vX.Y.Z -m "amdtop vX.Y.Z"
git show --stat vX.Y.Z
git push origin vX.Y.Z
```

Pushing the tag starts `.github/workflows/publish.yml`. The workflow:

1. checks out the tagged commit without retaining GitHub credentials;
2. verifies that the annotated tag, Cargo version, and dated changelog section
   agree;
3. runs rustfmt, Clippy, tests, and `cargo publish --dry-run` on Rust 1.88;
4. requests a temporary crates.io token through OIDC; and
5. runs `cargo publish --locked`.

The workflow's third-party actions are pinned to reviewed commit SHAs. Review
and update those pins deliberately when updating either action.

### Monitor Publication

- [ ] Find the publishing run and wait for it to succeed:

  ```sh
  gh run list --workflow publish.yml --limit 5
  gh run watch RUN_ID --exit-status
  ```

- [ ] If the job is waiting for the `release` environment, review and approve
      the deployment in GitHub.
- [ ] Confirm the new version and publisher on crates.io:

  ```sh
  curl -fsS https://crates.io/api/v1/crates/amdtop/X.Y.Z | jq
  ```

Do not create another token when OIDC authentication fails. First compare the
four trusted-publisher fields against the workflow and check that the GitHub
job uses the `release` environment. Correct configuration or transient errors
can be followed by **Re-run failed jobs**.

### GitHub Release

After crates.io publication succeeds, copy the matching changelog section into
a temporary notes file, review it, and create the GitHub Release from the
existing tag:

```sh
$EDITOR /tmp/amdtop-vX.Y.Z.md
gh release create vX.Y.Z \
  --verify-tag \
  --title "amdtop vX.Y.Z" \
  --notes-file /tmp/amdtop-vX.Y.Z.md
gh release view vX.Y.Z --json url,name,body
```

Do not leave the GitHub Release notes empty and do not ask GitHub to create a
new tag.

### Verify the Published Artifact

Registry indexing can take a few minutes. Once the version is visible:

```sh
tmp_root="$(mktemp -d)"
cargo install amdtop --version X.Y.Z --locked --root "$tmp_root"
"$tmp_root/bin/amdtop" --version
rm -rf "$tmp_root"
```

- [ ] Confirm the reported version is `amdtop X.Y.Z`.
- [ ] On an AMD system, launch the registry-installed binary and perform a
      short telemetry smoke test.
- [ ] Confirm the crates.io page, GitHub tag, GitHub Release, `Cargo.toml`, and
      changelog all identify the same version.
- [ ] Confirm the development tree is clean: `git status -sb`.

### Publish the AUR Update

Update the AUR immediately after the crates.io archive is downloadable and the
registry-installed binary has passed verification. A maintainer can run the
local tool interactively:

```sh
scripts/update-aur.sh X.Y.Z
```

A local deploying agent that has been explicitly authorized to finish the
release can provide the publication confirmation as an option:

```sh
scripts/update-aur.sh --publish X.Y.Z
```

`--publish` is deliberately unavailable in CI; it is an explicit instruction
to the local deployment agent, not unattended release automation.

The script:

1. requires the Cargo and requested versions to agree;
2. synchronizes the GitHub packaging mirror and verifies any existing AUR head;
3. downloads the published crates.io archive and computes its BLAKE2b checksum;
4. updates `PKGBUILD` and regenerates `.SRCINFO`;
5. checks REUSE licensing, builds and tests in a clean Arch chroot, runs
   `namcap`, and verifies the packaged executable's version;
6. shows the complete packaging diff;
7. commits and pushes first to the AUR and then to the GitHub mirror only after
   either `publish amdtop X.Y.Z-1` is typed exactly or `--publish` was supplied;
   and
8. waits for the AUR RPC to report the expected `X.Y.Z-N` version.

- [ ] Review upstream dependency, license, asset, and build-system changes; do
      not treat a passing script as approval for an automatic version bump.
      Confirm the sibling `PKGBUILD` uses the current upstream license and
      installs all applicable `LICENSE`, `NOTICE`, and attribution files.
- [ ] Resolve every `namcap` error and review each warning rather than applying
      its suggestions blindly.
- [ ] Confirm the package contains `/usr/bin/amdtop`, `LICENSE`, `NOTICE`,
      `THIRD_PARTY.md`, and expected pacman metadata, and that the binary
      reports `amdtop X.Y.Z`.
- [ ] Optionally install the built package and run the hardware smoke test
      before entering the publication confirmation.
- [ ] Confirm the displayed `PKGBUILD` and `.SRCINFO` version, dependencies,
      source URL, release number, and checksum.

Intentional edits already present in the sibling `PKGBUILD` are preserved by
the script. For a packaging-only change, leave `pkgver` unchanged, increment
`pkgrel`, regenerate `.SRCINFO`, and run the script; its confirmation will use
the resulting `X.Y.Z-N` release. Changes outside `PKGBUILD` and `.SRCINFO` are
rejected.

Confirm that the AUR indexed the update:

```sh
curl -fsS 'https://aur.archlinux.org/rpc/v5/info?arg[]=amdtop' |
  jq -e '.results[0] | {Name, Version, URLPath, OutOfDate}'
```

- [ ] Confirm the AUR reports the intended `X.Y.Z-N` and package metadata;
      do not mark the release complete while it still reports the prior version.
- [ ] Confirm the AUR and GitHub mirror point to the same commit.
- [ ] Confirm both the upstream and AUR working trees are clean.

A broken AUR recipe does not justify moving the upstream tag or yanking a
usable crates.io release; fix the recipe and publish a new `pkgrel`.

## Failures and Yanking

Do not move or reuse a public release tag. If source or metadata must change,
fix it and publish a new patch version.

If a published version is unusable, create a short-lived API token with only
the `yank` scope and the exact crate restriction `amdtop`, then run:

```sh
cargo yank amdtop --version X.Y.Z
```

Yanking does not delete the crate. Existing lockfiles can continue using it,
but new dependency resolution will avoid it. Undo an incorrect yank with:

```sh
cargo yank amdtop --version X.Y.Z --undo
```

Revoke the yank token when finished. The OIDC publishing token intentionally
cannot yank releases.
