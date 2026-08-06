# Changesets

Add one changeset for every user-facing npm package change:

```sh
pnpm changeset
```

The Changesets workflow opens a version pull request on `main`. Merging that
pull request publishes the changed public npm packages through npm Trusted
Publishing from this repository.
