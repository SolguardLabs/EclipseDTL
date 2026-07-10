# Security Model

EclipseDTL assumes assets, accounts, operators and routes are configured by an
internal control plane before batches are admitted. Runtime execution is scoped
to deterministic JSON scenarios and the Rust public API.

## Expected Invariants

- Batches settle only through registered routes matching source and target
  assets.
- Selected bids must satisfy fee, route and guarantee policy at admission.
- Vault transfers must preserve asset-level accounting.
- Operator fee accounts must exist before settlement.
- Fallback execution must use an admitted bid for the same batch.

## Automated Validation

Local CI runs formatting, build, Rust tests, Clippy and Node scenario tests:

```bash
bash scripts/ci.sh
```

The public tests cover bid admission, operator selection, guarantee attachment,
settlement receipts, balance movement and fallback settlement.

## Dependency Management

Dependabot is configured for Cargo, npm metadata and GitHub Actions. Lockfiles
are honored by CI for reproducible Rust builds.

## Review Scope

Primary review targets are:

- economic admission logic in `src/risk.rs`;
- bid scoring and lifecycle transitions in `src/auction.rs`;
- guarantee and exposure accounting in `src/operators.rs`;
- vault and fee movements in `src/settlement.rs`;
- scenario orchestration in `src/scenario.rs`.

Security reports should include the affected component, reproduction scenario,
observed accounting impact and proposed regression coverage.
