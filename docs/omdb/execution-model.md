# Native execution and ownership model

Status: proposed architecture with a bounded hosted execution example.
Date: 2026-11-10.

## The installed system

Use one logical database system per node. Link or compose the native boot,
hardware, storage and execution modules that the selected profile needs. Keep
one active authority for each resource scope and generation. Applications may
call Rust interfaces directly within the same admitted trust domain; isolation
and remote access require explicit interfaces with equivalent authorization.
Untrusted native programs need hardware-enforced memory and device isolation.
Wasm programs need a qualified runtime with bounded memory, execution fuel and
explicit imports. Compiling a program in Rust does not grant it resource authority.

The database holds programs, objects, current authority, resource reservations,
desired transitions, work intents, results and recovery state. Native code
implements memory access, instructions, device queues and rendering. A small
bootstrap and recovery path must initialize storage and remain usable before the
main database opens. The main database cannot be its own sole recovery mechanism.

Large immutable data may use separately allocated cell extents whose manifests
and ownership are transactional. Bytes must reach the promised durability before
their manifest becomes visible. The same product can therefore have one logical
namespace and several physical databases or extent stores, each with a declared
owner and recovery rule.

## One writer per consistency domain

The inspected redb revision already supports several writer processes through
`ConcurrencyMode::MultiWriter`. Only one write transaction is active against a
database at a time. Concurrent readers retain snapshots. `try_begin_write`
returns a contention error rather than waiting for the writer slot, but backend
I/O and other synchronization can still block. It is not a wait-free or async
execution guarantee. [Engine interface](../../src/db.rs),
[locking design](../design.md#multi-process-concurrency),
[synchronization](../../src/sync.rs).

Retain this serial commit order within each lane. A lane owns a consistency
domain, such as one aggregate, tenant partition or resource allocation group.
Independent lanes use independently backed databases and can commit in parallel.
Several tables or several handles to the same database do not create independent
writers.

Begin with a small node catalog and a small number of work lanes. Partition by
atomic invariants and ownership, then measure contention. Bind a lane to a CPU or
NUMA domain only as a scheduling decision. Do not persist CPU-core identity as the
database's logical identity. Tiny devices may use one lane; larger devices may
add lanes when their throughput benefit exceeds cache, memory, synchronization
and recovery costs.

Routing needs a versioned map. Moving a domain requires a durable migration
decision, fencing the old owner, transferring state and receipts, activating the
new routing generation, and retaining retry lookup. A retry must never become a
new operation merely because the domain moved.

The catalog must not become a writer that every lane needs on every operation.
Use recoverable routing generations and validate cached routes at the receiving
lane. Bound long-lived snapshots, command queues, conflict retries and output
sizes so one stalled operation cannot retain resources indefinitely.

The writer is a bounded state-transition engine. Its command can select work,
reserve capacity, change objects and publish an intent. CPU/GPU execution,
compilation, model inference, network transfers and physical device actions run
outside the transaction. Their outcomes return as new authorized transitions.

### Admission and atomic commit

Commands identify the authenticated principal, scope, lane and routing generation,
authority epoch, stable operation ID, complete canonical input, expected state,
policy version and bounded resource requirements. A caller-supplied owner ID is
not authentication. The hosted example uses synthetic trusted caller IDs and
does not implement an authentication protocol.

For each command:

1. Check current authenticated authority, scope and epoch.
2. Look up a retained receipt. An identical original request returns its retained
   result after the current authorization check. Reuse of the operation ID for
   different input fails. Do not rerun original mutation preconditions against
   the state that the completed operation already changed.
3. For new work, validate the complete read set, range dependencies, routing
   generation, domain invariants and available resource reservation. Single-key
   version checks are sufficient only for a contract that reads that key alone.
4. Commit authoritative state, reservation, original request, result and external
   effect intent together in one lane transaction.
5. Acknowledge the state commit only after its selected durability is established.
   Report external completion separately.

Use checked arithmetic, bounded requests and explicit backpressure. Retained
receipts and pending intents also need quotas, retention policy and a retry
expiry contract. Deleting a receipt without such a contract permits old requests
to execute again.

The [example](../../examples/omdb_lanes.rs) implements only its narrow counter and
effect-intent contract. It is not a general optimistic-concurrency implementation
or a complete resource scheduler.

## Hardware effects and revocation

Discovery records facts about devices. An owner grant authorizes an operation.
Keep these distinct. A hardware description includes topology, medium or device
class, driver and firmware identity, supported operations, health, power and
thermal limits, isolation requirements, generation and qualification evidence.

The receiving driver validates a current fence before accepting an effect.
Revocation must also quiesce outstanding queues and DMA mappings or reset the
device when necessary. An epoch stored in a database cannot by itself stop an
already queued DMA write. Device reset and IOMMU groups can contain several
functions that must share an ownership decision. See the
[hardware inventory and requirements](hardware-upstream.md).

An outbox provides atomic durable intent with database state. It cannot make an
uncooperative external device exactly-once. If a crash occurs after an effect but
before its result is stored, use a device-supported idempotency key, query and
reconcile the device, or retain an uncertain outcome. Never infer success from
the absence of an error report.

Effects also need ordering. A valid authority epoch does not prevent an older
desired value from arriving after a newer one. Define per-resource sequencing or
have the receiver reject stale versions under the resource's operation contract.
The hosted example persists intents but has no device receiver or ordered effect
dispatcher. Intents retained across an authority change require a separately
authorized reconciliation path.

Interrupts, DMA completion, watchdogs, emergency cooling, display scanout and
network progress must not acquire the database writer for every event or depend
on a remote quorum. Persist policies and transitions; run timing-sensitive work
in bounded native loops with independently enforced safety limits.

## Raw storage adapter contract

`no_std` requires `alloc`, `panic = "abort"`, supported atomics and
`experimental-api-5` with default features disabled. Current optional features
have additional requirements. This removes the standard-library dependency from
the engine; it does not provide boot, allocation, interrupts or drivers.
[Pinned upstream requirements](https://github.com/cberner/redb/blob/9273718b3ba5282580ca9035b0fb95b5bf13988f/README.md).

The native adapter must implement the actual [StorageBackend](../../src/db.rs)
contract, including bounds, zero-filled growth, logical length, error propagation,
thread safety and persistence ordering. It must own a non-overlapping admitted
extent. Reopening that extent must recover its length and allocation identity.
Raw sector read-modify-write must not introduce torn updates or overwrite another
lane's data. Alignment, DMA coherence, cache flushes, sector atomicity and power
loss behavior are properties to qualify on the selected device.

The backend must also preserve exclusive ownership across independently opened
handles. If its lock methods are unsupported, the surrounding runtime must stop
two database instances opening the same extent. A successful reopen cannot infer
ownership merely from a device pathname. Persist logical-length changes and
zero-initialization metadata when `sync_data` is called, including the engine's
resize path that does not first flush its ordinary write buffer.
[Storage interface](../../src/db.rs),
[resize synchronization](../../src/tree_store/page_store/cached_file.rs).

In particular, redb's two commit slots and controlling byte can occupy a shared
physical sector. Translating a byte update into a torn whole-sector rewrite can
damage both slots. The adapter needs a demonstrated atomic-sector guarantee or
its own recoverable update scheme. DMA completion alone does not prove persistence;
flushes must cover relevant software, controller and volatile device caches.
For example, Linux's documented FUA completion covers the flagged request, while
its preflush establishes persistence of prior completed writes. A native backend
must prove the corresponding semantics for its selected controller, not assume
that FUA or a CPU memory barrier drains every earlier write.
[Write-cache semantics](https://docs.kernel.org/block/writeback_cache_control.html).

Use the storage engine's existing recovery and checksum behavior. If the backend
cannot meet assumptions of the selected commit protocol, use an appropriate
qualified protocol or reject the profile. The engine's `two_phase_commit` option
concerns local persistence; it is not a cross-lane distributed transaction
coordinator. [Commit design](../design.md).

Small-device profiles must set an explicit cache and allocation budget. The
current default cache is 1 GiB and `no_std` locks spin. A cooperative single-core
executor must not yield while holding database guards or call the database from
an interrupt handler. A blocking backend must have an independent completion
path. NOR flash also needs erase-block and wear handling. Some constrained
devices can supply a bounded resource-agent role before they can host the full
engine. Record that distinction in their capability profile.
[Builder defaults](../../src/db.rs), [freestanding locks](../../src/sync.rs),
[target requirements](../../README.md#no_std).

## Pooling and compatibility

One catalog can describe DRAM, persistent memory, SSD, HDD and remote storage.
Preserve medium-specific durability, latency, addressability, DMA, endurance and
failure-domain semantics. RAM may cache durable objects or hold explicitly
volatile objects. Replicating RAM does not establish survival of correlated power
loss.

Pool only owner-granted capacity. Discovery alone must not claim a mounted disk,
format a device or overwrite existing content. Admission, draining, repair,
reference retirement and release are explicit state transitions. Owners retain
reserves and the ability to revoke future sharing under a recoverable policy.

Use versioned uniform encoded storage cells. Cell size and erasure-code width are
separate decisions. A stripe with `k` data cells and `m` parity cells has nominal
coding expansion `(k + m) / k`; headers, padding, manifests, temporary repair and
metadata add to actual use. Select redundancy by fault budget and failure domains,
and select cell size by measured workload. Neither choice implies fixed 4+2.

Expose S3 compatibility through an object-protocol adapter. `s3s` supplies a Rust
S3 service interface; RustFS and Garage supply object-storage implementation
experience; OpenDAL and `object_store` supply access adapters. Their respective
interfaces do not by themselves provide a bare-metal raw-device pool. Inspect
exact licenses, runtime requirements and supported operations before reuse.
[s3s](https://github.com/s3s-project/s3s),
[RustFS](https://github.com/rustfs/rustfs),
[Garage](https://github.com/deuxfleurs-org/garage),
[OpenDAL](https://opendal.apache.org/docs/rust/opendal/),
[object_store](https://docs.rs/object_store/latest/object_store/).

Local calls can bypass HTTP serialization. Remote sessions must authenticate
peers and current scope, preserve the required encryption profile and use
outbound connections/relays where the deployment requires no public inbound
ports. A transport library does not establish authorization, application
confidentiality, post-quantum coverage or replicated durability by itself.

## Distributed authority and resource fairness

Keep state requiring atomicity in one lane when possible. Cross-lane work uses
an explicit durable workflow or a recoverable atomic commit protocol. Sequential
commits and outboxes alone do not create multi-database atomicity.

Replicated authority is scoped to a shard or trust domain. Raft is a possible
crash-fault consensus mechanism for admitted replicas; it requires a quorum and
does not tolerate arbitrary malicious replicas. Untrusted storage contributors
must not gain identity or accounting authority merely by storing ciphertext.
[Raft paper](https://raft.github.io/raft.pdf).

Ownership, authorization, contribution evidence, entitlement, demand and placement
utility have distinct meanings. A contribution earns an entitlement under an
explicit policy; it does not transfer someone else's hardware ownership. Preserve
resource quantities with units and measurement provenance. CPU time, GPU work,
memory byte-seconds and durable-storage byte-seconds have no universal physical
exchange rate.

Weighted dominant-resource fairness is a candidate scheduling baseline under its
stated capacity and demand model. It does not solve receipt fraud, identity
splitting, heterogeneous placement, indivisible jobs or trust requirements.
Verify useful delivered work, deduplicate receipts, retain policy versions and
measure fairness over an explicit interval. Locality selects eligible placements
without silently changing earned entitlement.
[DRF paper](https://www.usenix.org/legacy/event/nsdi11/tech/full_papers/Ghodsi.pdf).

## Evidence and optimization

Distinguish source reuse, successful compilation, hosted execution, virtual-machine
boot, power-loss recovery and physical-device qualification. Link every result to
the actual source, compiler, features, backend and failure model.

Measure complete operations under equal security and durability requirements.
Record queue delay, p50/p95/p99 latency, throughput, memory per lane, physical
writes, recovery time, repair traffic and energy when measurable. Independent
lanes should be added because measured contention warrants them. No current
benchmark establishes universal speed, optimality or complete hardware support.
