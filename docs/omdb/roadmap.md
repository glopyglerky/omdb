# Implementation sequence and acceptance

The target is a native database-led resource system. These gates keep that target
connected to working code. A gate is complete only when its specified behavior
has been executed against an identified implementation.

| Gate | Deliverable | Acceptance | Current state |
| --- | --- | --- | --- |
| OMDB-001 | Independent state lanes | Real transactions, same-lane contention, progress in another lane, owner/epoch checks, atomic state/receipt/intent, matching retry, changed-input rejection, abort/reopen | Hosted example added; see qualification record |
| OMDB-002 | Runtime engine adoption | An existing stored application runs against this pinned fork, preserves its objects/results and survives restart and verified export/restore | Planned; consumer migrations not performed here |
| OMDB-003 | Native raw block backend | Bounds, exclusive extents, logical length, zero-filled growth, short I/O/error propagation, barriers and crash recovery pass a deterministic fault-injection suite | Planned |
| OMDB-004 | First bootable profile | Pinned Rust boot/runtime and VirtIO backend open the actual database, execute a stored operation, reboot on the same virtual disk, recover and use independent rescue | Planned |
| OMDB-005 | Native hardware control | Enumerate capabilities, grant a scoped operation, enforce its epoch at the receiver, revoke queued work and prove DMA/reset ownership on the selected profile | Planned |
| OMDB-006 | Responsive UI and safety | Status/input/display remain responsive under database contention; watchdog and thermal fallback survive a database outage | Planned |
| OMDB-007 | Local storage pool | Two admitted synthetic devices with distinct medium classes; hot add/drain/remove, reservations, uniform cells, private manifests, repair and write-amplification measurements | Planned |
| OMDB-008 | Replicated authority | Explicit replica membership, durable quorum decisions, snapshot/log recovery, old-leader fencing, partition tests and unavailable-quorum behavior | Planned |
| OMDB-009 | Owner-controlled federation | Authenticated owner offers, revocation, reservations, private data placement, failure-domain verification and no-secret execution on untrusted nodes | Planned |
| OMDB-010 | S3 interoperability | Bounded authenticated PUT/GET/HEAD/DELETE/LIST, conditional operations, range reads, multipart retry/abort and named-client conformance | Planned |
| OMDB-011 | Contribution and fairness | Typed metered service, receipt verification/deduplication, policy-versioned entitlement, bounded allocation and adversarial identity/receipt tests | Planned |
| OMDB-012 | Product migrations | Each consumer preserves public behavior, privacy, custody, atomicity, restart and rollback through its actual persistence seam | Planned |
| OMDB-013 | Second architecture and physical devices | Repeat the native contract on a second architecture, then qualify named controller/firmware profiles with real power-cut and device-reset evidence | Planned |

The first native integration should reuse an existing qualified platform and
application runtime where available. The [upstream inventory](hardware-upstream.md)
identifies ArceOS/VirtIO and a minimal rust-osdev boot path as candidate integration
routes. It does not require replacing another working platform merely to use a
different donor.

Dependencies are practical. OMDB-001 informs OMDB-002 and OMDB-003. OMDB-003 and a
selected platform enable OMDB-004. Device enforcement in OMDB-005 is required
before physical pooling or unrestricted remote hardware work. OMDB-008 and
OMDB-009 are required before claiming global authority or durable global pooling.
Protocol adaptation can proceed earlier against a qualified local object store.

Preserve a capability matrix covering storage, memory, CPU, accelerators, network
and radios, PCI/USB and embedded buses, power, cooling, lighting, audio, display,
input and user interfaces. Record each actual device function as discovered,
candidate, implemented, qualified, degraded or unavailable. Selection happens
inside the installer/runtime; the user should not need to choose a donor OS.

## Required failure experiments

- Crash before/after durable state commit and before/after external effects.
- Reuse an operation ID with changed input and retry after authority revocation.
- Deliver newer and older effect versions out of order and verify the resource's
  sequencing contract prevents a stale update from undoing the newer result.
- Saturate one writer while another lane and the safety path make progress.
- Interrupt lane migration and replay a stale routing generation.
- Inject short reads/writes, failed flushes, torn sectors and hot removal.
- Lose the stated number of independent storage failure domains and verify that
  the acknowledged manifest remains retrievable, or that degraded state is honest.
- Partition replicas, recover an old leader and attempt stale resource use.
- Restore an older backup and ensure it cannot revive revoked device authority.
- Submit oversized work, exhausted budgets and fabricated contribution receipts.

A process-exit recovery test proves only that failure model. It does not simulate
loss of host RAM, a volatile controller cache, a torn sector or a correlated
power failure. Record those experiments separately.

Performance comparisons must include authorization, encryption, requested
durability, maintenance and recovery costs. Tests of a narrow contract do not
constitute a proof of a complete operating system or a globally fair economy.
