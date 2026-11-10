# omdb direction and evidence

omdb is a redb fork developing a database-led native resource and execution
system. The target is one coherent installation that owns its admitted resources,
persists application state and work, executes bounded programs, and exposes
compatible object and application interfaces.

The current library is an embedded database. The first fork-specific code is the
[hosted execution-lane example](../../examples/omdb_lanes.rs). It exercises real
database transactions and recovery, but does not boot a machine, drive hardware,
authenticate remote users, or pool devices across nodes.

## Start here

- [Execution and ownership model](execution-model.md).
- [Rust-first upstream hardware inventory](hardware-upstream.md).
- [Implementation sequence and acceptance gates](roadmap.md).
- [Qualification record](qualification.md).
- [Upstream storage design](../design.md).

The engine baseline is upstream redb commit
[`9273718b3ba5282580ca9035b0fb95b5bf13988f`](https://github.com/cberner/redb/commit/9273718b3ba5282580ca9035b0fb95b5bf13988f).
The Cargo package remains `redb` to retain existing callers and upstream source
organization. The fork does not change the engine file format in this slice.

## Delivery rules

Build a usable vertical slice and retain its recovery proof before expanding its
claimed scope. A source catalog, matching device ID, successful cross-compile,
or database row does not establish a working hardware capability.

Preserve donor notices and exact source identities. Keep private application and
policy implementations in their existing private repositories until their owner
explicitly authorizes publication. A shared engine does not require publishing
all of its consumers.

The complete target includes CPU and accelerator execution, memory, storage,
network and radio, device buses, power and cooling, RGB, audio, displays, input,
accessible interfaces, embedded controls and application UIs. An implementation
must report unavailable capabilities rather than silently dropping them from the
inventory. No universal hardware-support or performance claim is made here.
