# Releasing

This crate publishes to [crates.io](https://crates.io) as `demografix`. Releases run
from `.github/workflows/release.yml` when a maintainer pushes a `vX.Y.Z` tag.

## One-time setup

The crate name `demografix` must be owned by the Demografix account before the first
release. Publishing the first version claims the name; after that, only owners can
publish new versions.

### Trusted Publishing (preferred)

crates.io supports OIDC Trusted Publishing, so the workflow does not store a long-lived
API token. The release job exchanges the GitHub Actions OIDC token for a short-lived
crates.io token through `rust-lang/crates-io-auth-action`.

Configure the trusted publisher once, from an account with owner rights on the crate:

1. Sign in to crates.io and open the crate settings for `demografix`
   (Account Settings -> the crate -> Trusted Publishing). The first version must be
   published manually with a token before Trusted Publishing can be attached, since the
   crate must exist.
2. Add a GitHub Actions trusted publisher with:
   - Repository owner: `DemografixGenderize`
   - Repository name: `demografix-rust`
   - Workflow file name: `release.yml`
   - Environment: `release`
3. Create a GitHub Actions environment named `release` in the repository settings
   (Settings -> Environments). The publish job references it. Add required reviewers
   there if you want a manual approval gate before each publish.

No repository secret is needed for the OIDC path. The release job already requests the
`id-token: write` permission it needs.

### Token fallback

If Trusted Publishing is not configured, publish with a stored API token instead:

1. On crates.io, open Account Settings -> API Tokens and create a token scoped to
   publishing the `demografix` crate.
2. In the GitHub repository, add it as the secret `CARGO_REGISTRY_TOKEN`
   (Settings -> Secrets and variables -> Actions).
3. In `release.yml`, remove the `rust-lang/crates-io-auth-action` step and change the
   publish step to read the secret directly:

   ```yaml
   - name: Publish to crates.io
     run: cargo publish
     env:
       CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
   ```

   The `id-token: write` permission and the `release` environment are not required for
   the token path, though the environment is still a useful approval gate.

## Cutting a release

1. Bump `version` in `Cargo.toml` to the new `X.Y.Z`. Run `cargo build` so `Cargo.lock`
   updates to match.
2. Commit the change:

   ```sh
   git add Cargo.toml Cargo.lock
   git commit -m "Release vX.Y.Z"
   ```

3. Tag and push the tag:

   ```sh
   git tag vX.Y.Z
   git push origin vX.Y.Z
   ```

Pushing the tag runs the release workflow. It checks that the tag version matches the
`Cargo.toml` version, builds, runs the tests, publishes to crates.io, then creates a
GitHub Release for the tag.

## Notes

- `cargo publish` is irreversible. A published version cannot be deleted, only yanked.
  A yank hides the version from new dependency resolution but does not remove it, and
  the version number can never be reused. Confirm the version and contents before you
  push the tag.
- The tag version (without the leading `v`) must equal the `Cargo.toml` version, or the
  release job fails before publishing.
