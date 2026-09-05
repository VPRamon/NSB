# `nsb` public API lifecycle

The `nsb` public API is currently **not frozen**. Public signatures may still
change while the first release surface is being corrected and reviewed.

CI always keeps the compatibility-only symbol guard active. The snapshot and
historical SemVer checks become authoritative only after the freeze marker
`crates/nsb/api/API_FROZEN` is committed.

To freeze the API:

1. Review the intended public surface.
2. Add `crates/nsb/api/API_FROZEN`.
3. Generate `public-api.txt` with:

   ```bash
   cargo run --locked -p nsb-public-api-gate -- --write
   ```

4. Commit the marker and generated snapshot together.

That freeze commit establishes the baseline. Subsequent changes must keep the
snapshot synchronized and pass the historical `cargo-public-api` compatibility
gate.

Policy: [`docs/developer-guide/public-api.md`](../../../docs/developer-guide/public-api.md).
