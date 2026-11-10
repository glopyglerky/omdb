# Execution-lane qualification

Scope: the experimental hosted example in
[`examples/omdb_lanes.rs`](../../examples/omdb_lanes.rs). The upstream engine
implementation and file format are unchanged by this slice.

## Executed local checks

On 2026-11-10, the hosted example's 10 tests and its two-lane demonstration passed
using the BlueFe/Lustr `preview-2026.11.09-2` compiler package on Linux GNU x86_64.
Its package provenance identifies BlueFe source
`1b74388e196ee8813f38ba9eeb6d4ab43b6b1071` and Lustr source
`07470b20464bf097c3c88983124aff56c5e44f9a`. A host loader adapter preserved the
compiler package and selected its matching sysroot. This is a native hosted
test, not a Lust-subset compiler or bare-metal execution claim.

The targeted commands, run through that package's Lustr coordinator, were:

```sh
lustr --bluefe-root "$BLUEFE_ROOT" test -p redb@4.5.0 --example omdb_lanes --features experimental-api-5
lustr --bluefe-root "$BLUEFE_ROOT" run -p redb@4.5.0 --example omdb_lanes --features experimental-api-5
```

The tests exercise:

- A busy lane rejects immediate write admission while another database progresses.
- The original request and result survive a matching retry; changed input cannot
  reuse the operation ID.
- Current owner, lane and epoch checks apply before a retained result is returned.
- State, receipt and effect intent roll back together when a transaction aborts.
- Checked arithmetic and bounded command values reject invalid work.
- Pending effect state survives reopening and effect-ledger completion is
  idempotent under the current authority.
- A child process exits without running destructors before a staged commit or
  after its durable commit. Reopening observes the corresponding old or new
  state, receipt and effect intent.
- Ownership transfer fences the old owner in the ledger and retains old intents
  for later reconciliation.

## Repository gate

The [omdb foundation workflow](../../.github/workflows/omdb-foundation.yml) runs
the repository-required `just test`, the executable example, and
`just build_no_std` against the exact candidate revision. Its run result is the
authority for whether that revision passed these checks; the existence of this
file or workflow is not a passing result.

`just test` retains the existing license/advisory audit, formatting, Clippy and
test commands and the repository's isolated Podman build path. The freestanding
check compiles the existing engine for `thumbv7em-none-eabihf` without `std`.
The workflow uses the repository's pinned Rust toolchain. Dependency locks are
resolved by the upstream recipes rather than committed, so the workflow and
local compiler runs are distinct dependency/build observations.

## Limits

The example trusts synthetic caller IDs supplied by its embedding code. It has
no authentication protocol, scheduler, resource allocator, remote worker or
hardware driver. A fetched intent does not stop a physical device from accepting
a stale epoch or sequence. Actual receivers need fenced ownership, ordered or
idempotent operations and a reconciliation path.

The child-process tests establish a bounded process-crash/reopen result on a
hosted filesystem. They do not simulate torn sectors, controller cache loss,
power failure or distributed partitions. A `no_std` compile does not establish a
bootable system or a qualified native storage adapter.

No throughput or latency benchmark was run. No improvement in speed, physical
durability, global fairness or universal device support follows from these tests.
