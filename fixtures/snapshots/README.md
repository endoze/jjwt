# Snapshot Provenance

## Fixture source

`fixtures/myapp.wt.toml` was copied verbatim from
`/Users/endoze/Projects/myapp/.config/wt.toml` (real project file).
It uses only the `branch` variable with the `hash_port` and `sanitize` filters —
no extra `RenderContext` fields were needed.

## Worktrunk cross-validation

`worktrunk` was **NOT** available at seeding time (`which worktrunk` returned nothing;
`wt` resolved to an unrelated tool).

**Snapshots represent jjwt's current rendering. They were NOT cross-validated against
real worktrunk because worktrunk was not available at seeding time.**

## Re-seeding / cross-validation instructions

Once worktrunk is available, re-validate as follows:

1. For each branch in `BRANCHES` (`feat-port-webhook-to-rust`, `main`, `bug-foo`):
   - Run `wt` against the `fixtures/myapp.wt.toml` config for that branch.
   - Capture the rendered output for each template section.
   - Compare byte-for-byte with the corresponding snapshot in
     `tests/snapshots/template_fidelity__myapp_<branch>.snap`.

2. If divergence is found, stop and investigate — the filter or template
   implementation has drifted from real worktrunk.

3. To regenerate snapshots after a deliberate change:
   ```bash
   INSTA_UPDATE=always cargo test --test template_fidelity
   cargo insta review   # optional: review interactively
   ```

## Snapshot files

Located in `tests/snapshots/` (insta default):

- `template_fidelity__myapp_feat-port-webhook-to-rust.snap`
- `template_fidelity__myapp_main.snap`
- `template_fidelity__myapp_bug-foo.snap`
