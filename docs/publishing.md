# Publishing SDKs

The public packages are `@lenso/app`, `@lenso/sqlite`, `@lenso/console`, `lenso-app` and `lenso-sqlite`. Examples and the repository root are private. All SDK packages use MIT.

## Verified release

On 2026-11-13, [workflow 43432833493](https://github.com/LioRael/lenso/actions/runs/43432833493) published the following versions from signed commit `410d6a305ea4d0793ce1e63c9f310797ba733151`:

| Registry | Package | Version |
| --- | --- | --- |
| npm | [@lenso/app](https://www.npmjs.com/package/@lenso/app/v/0.1.1) | 0.1.1 |
| npm | [@lenso/sqlite](https://www.npmjs.com/package/@lenso/sqlite/v/0.1.1) | 0.1.1 |
| npm | [@lenso/console](https://www.npmjs.com/package/@lenso/console/v/3.0.0) | 3.0.0 |
| crates.io | [lenso-app](https://crates.io/crates/lenso-app/0.1.1) | 0.1.1 |
| crates.io | [lenso-sqlite](https://crates.io/crates/lenso-sqlite/0.1.1) | 0.1.1 |

All five packages have a Trusted Publisher for `LioRael/lenso`, workflow `publish.yml`, without an environment restriction. These are verified OIDC publications, not just saved configurations. npm generated SLSA provenance attestations; crates.io records the GitHub repository, workflow run and source SHA in each version's trusted publishing data. The new app/sqlite packages were first bootstrapped at 0.1.0 with maintainer authentication, then 0.1.1 verified the tokenless workflow. The Console package's first release from this repository is 3.0.0.

`@lenso/console` 3.x is the headless client and compiled page registry in this repository. Its 2.x releases belong to the previous Console host and have a different API. Existing host consumers should stay on 2.x until they explicitly migrate; installing 3.x does not provide the previous host application.

## Release

Update the intended package versions and internal dependency ranges, run the existing checks, then commit and push to `main`. Versions can advance independently. Dispatch **Publish SDKs** (`.github/workflows/publish.yml`) on `main`, choosing `npm`, `crates` or `both`. The workflow runs language checks before uploading packages in dependency order. Published versions are skipped so a partially completed release can be resumed; registry failures other than a missing version stop the release.

Both jobs request GitHub OIDC credentials. npm authenticates automatically; crates.io uses `rust-lang/crates-io-auth-action`, which revokes its short-lived token when the job ends. No npm or crates.io long-lived secret is needed by this workflow. Successful npm OIDC uploads also include provenance.

## First publication and trust configuration

Each new package must first be published with an authenticated maintainer account. After that, configure its trusted publisher using:

| Field | Value |
| --- | --- |
| Provider | GitHub Actions |
| Owner | `LioRael` |
| Repository | `lenso` |
| Workflow filename | `publish.yml` |
| Environment | Empty (the workflow does not use a GitHub environment) |
| npm permission | Allow direct `npm publish` |

For npm, an authenticated maintainer with 2FA can run, once for each package:

```sh
npm trust github @lenso/app --repo LioRael/lenso --file publish.yml --allow-publish --yes
npm trust github @lenso/sqlite --repo LioRael/lenso --file publish.yml --allow-publish --yes
npm trust github @lenso/console --repo LioRael/lenso --file publish.yml --allow-publish --yes
```

For crates.io, use each crate's **Settings → Trusted Publishing** with the same fields. A saved configuration is distinct from a verified OIDC publication: run a new version through the workflow to verify it. npm requires the first successful trusted publication within two days of creating the configuration.

See the official [npm trusted publishing documentation](https://docs.npmjs.com/trusted-publishers/) and [crates.io trusted publishing documentation](https://crates.io/docs/trusted-publishing).
