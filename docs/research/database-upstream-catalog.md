# Database extension upstream catalog

Reviewed: 2026-11-10, America/Denver.

This catalog covers 47 upstream projects and forks for vector search, graph and
relational queries, tags, full-text search, incremental views, and storage or
replication components. It is a source-reuse and comparison inventory, not a list
of dependencies to install. It is not an exhaustive inventory of every database.

The [machine-readable references](database-candidates.json) retain exact commits,
source paths, review depth, selection status, and proposed comparison cases.
The [comparison plan](database-comparison-plan.md) defines those cases. No
comparative database tests or benchmarks were run for this catalog.

## Decision

Keep the existing RuVector source import as the baseline. It is actively
maintained, but the evidence does not establish it as the best implementation of
every database capability. Compare a small set by the operation being added:

| Operation | First references | Why these first |
| --- | --- | --- |
| Vector indexing over omdb | RuVector, Microsoft DiskANN3, shodh-redb | Existing imported implementation; a storage-provider-oriented Rust ANN library; a transaction-bound vector implementation in another redb fork. |
| Relational queries over omdb | GlueSQL, ContextDB | An existing redb SQL adapter and a concrete shared relational/graph/vector commit path. |
| Tags and exact filters | redb tables/multimaps, roaring-rs | Keep exact small queries simple; compare compressed bitmap intersections when filtering cost justifies them. |
| Persistent property graph | RuVector graph, ContextDB, NopalDB, Grafeo | Compare query coverage, transaction ownership, update cost and recovery, not just graph traversal speed. |
| Full-text and hybrid retrieval | Tantivy, Corvid, UQA Engine | Reusable lexical indexing and examples of combining text, property and vector results. UQA has a native C dependency boundary. |
| Standalone SQL/vector alternative | Stoolap, Turso | Different relational engines with different vector capabilities; they are alternatives to the storage stack, not redb plug-ins. |
| Specialized retrieval baseline | Qdrant, LanceDB | External comparators for filtered vector and embedded columnar retrieval. Neither is a drop-in all-model redb extension. |
| Analytical SQL and incremental views | DataFusion, Differential Dataflow, Timely Dataflow | Query execution and incremental computation libraries. They do not supply omdb transactions or durable replay automatically. |

These are review priorities, not benchmark rankings. One small comparison per
requested capability is preferable to compiling the entire catalog.

## RuVector maintenance and integration boundary

The imported snapshot is
[`39a104e50388d41837f86e4001096a01927e1728`](https://github.com/ruvnet/RuVector/tree/39a104e50388d41837f86e4001096a01927e1728).
Its [import record](../../upstream/ruvector-import.json) remains authoritative for
which source is present in this repository.

At review, upstream had November 10 security/dependency maintenance, package
publication and bug-fix activity. A sampled recent substantive revision had a
[successful workspace run](https://github.com/ruvnet/RuVector/actions/runs/42904857656)
with 18 completed jobs. In a bounded sample of 50 recent main-branch commits,
31 were attributed to the primary account, 17 to automation, and two to another
account. That supports active maintenance with concentrated recent contribution;
it does not establish a lifetime contributor count, independent audit or support
commitment. Some lint steps in upstream CI are non-blocking. A green run is not
proof of every README capability or every optional profile.

The pinned workspace requests redb 2.1, while this fork's package is redb 4.5.0.
More importantly, the inspected
[vector storage](https://github.com/ruvnet/RuVector/blob/39a104e50388d41837f86e4001096a01927e1728/crates/ruvector-core/src/storage.rs)
and
[graph storage](https://github.com/ruvnet/RuVector/blob/39a104e50388d41837f86e4001096a01927e1728/crates/ruvector-graph/src/storage.rs)
own database handles and commit operations separately. Using redb in both does
not make one caller-owned transaction span vectors, edges and application rows.
That is an adapter question, not a reason to retest redb's whole B-tree engine.

The import does not wire those crates into omdb's root build. Upstream feature
profiles also include native-library and hosted-runtime boundaries. Do not infer
a C-free dependency closure, bare-metal support, SQL completeness or one atomic
multimodel database from the source import alone.

## Extensions already using redb

Versions below are the dependency requirements observed at the pinned source,
not promises of API or on-disk compatibility with omdb. Full pins are in the JSON
manifest. License labels describe the reviewed component, not every dependency.

| ID / upstream | Observed role and storage | Useful code and main boundary |
| --- | --- | --- |
| `ruvector` / [RuVector](https://github.com/ruvnet/RuVector) | MIT root; vector, graph and related crates; redb 2.1 workspace requirement | Imported baseline. Select concrete crates and profiles; separate storage owners do not imply one multimodel transaction. |
| `contextdb` / [ContextDB](https://github.com/context-graph-ai/contextdb) | Apache-2.0; SQL, graph and vector engine | Released 1.0.0 uses redb 3. Its persistence implementation commits rows, graph adjacency and vector records in one redb transaction. Development uses its own redb 4.1 fork and a newer v2 tree; compare release and development separately. |
| `uqa-engine` / [UQA Engine](https://github.com/cognica-io/uqa-engine) | Apache-2.0 from 0.6.2; SQL, graph, BM25, ANN/hybrid; redb 4.1 provider | Broad query reference. Current engine/parser manifests still bring SQLite/SQLCipher and a C PostgreSQL parser even with the redb provider. Earlier releases had different licensing. |
| `gluesql` / [GlueSQL](https://github.com/gluesql/gluesql) | Apache-2.0; SQL parser, planner, executor and redb 2.6 adapter | Focused SQL-layer candidate. Existing transaction/index interfaces save work, but the adapter owns its database and is not an already-integrated caller-transaction adapter for omdb 4.5. |
| `nopaldb` / [NopalDB](https://github.com/sharop/nopaldb) | MPL-2.0 library; property graph/NQL, temporal features, optional ANN/full-text; redb 4.1 | Graph and hybrid query donor. Own WAL and separate HNSW persistence must be reconciled with the intended commit boundary. Not a general relational SQL engine. |
| `corvid` / [Corvid](https://github.com/corvid-db/corvid) | MIT; redb 4.1, disk HNSW, BM25, properties, graph edges and lookup joins | Compact combined implementation. Graph and join scope is narrower than a general SQL/Cypher engine. Inspected latest commit/release was September 3. |
| `shodh-redb` / [shodh-redb](https://github.com/varun29ankuS/shodh-redb) | Apache-2.0; redb-derived storage, IVF-PQ, blob tags and causal relationships | Transaction-bound indexing and an explicit `no_std` configuration are relevant. Causal links are not a complete graph query language. This is a storage fork, not a plug-in; no target build was run here. |
| `native-db` / [native_db](https://github.com/vincent-herlemont/native_db) | MIT; typed objects, secondary indexes, migrations and watches; redb 3.0.2 | Record/schema-layer reference. No general SQL, graph or ANN engine. Inspected default-branch commit is from October 2025; repository push time alone would overstate engine maintenance. |
| `aresadb` / [AresaDB](https://github.com/yoreai/aresadb) | MIT; SQL-facing, graph/full-text/vector source; redb 2.x | Broader but lower-priority reference. Inspected vector code includes simplified HNSW-like behavior and brute-force filtered search; alpha distributed packages are not proof of an integrated production stack. |
| `parsnip` / [Parsnip](https://github.com/omar16100/parsnip) | MIT OR Apache-2.0; redb 2.2 application/entity store | Useful entity/relation/tag examples. Linear vector scans and application-specific project scans are not an ANN or general query engine. |
| `srvdb` / [SrvDB](https://github.com/Srinivas26k/srvdb) | Apache-2.0; embedded vector store using redb and separate vector persistence | Reopen/delete/compaction reference. README and manifests inspected, not its durability implementation; current source requires Rust 1.99 and lists SimSIMD. |
| `zvec-rs` / [zvec-rs](https://github.com/thedonmon/zvec-rs) | Manifest declares MIT; embedded vector database; redb 2 | Small additional comparison candidate. Manifest/source discovery only; license files, maintenance depth and persistence semantics still need review. |

ContextDB's shared commit is visible at
[`persistence.rs`, released 1.0.0](https://github.com/context-graph-ai/contextdb/blob/e6cf60cf972148bbbfe5a002151f40d191e551b9/crates/contextdb-engine/src/persistence.rs#L322-L505).
The current development pin is deliberately recorded separately in the manifest.
UQA's [licensing history](https://github.com/cognica-io/uqa-engine/blob/1ae12f023c3c4b2733fc60f6051992d3c342c8d2/LICENSING.md)
and [engine manifest](https://github.com/cognica-io/uqa-engine/blob/1ae12f023c3c4b2733fc60f6051992d3c342c8d2/crates/uqa-engine/Cargo.toml)
resolve two easy mistakes: treating its old AGPL label as current, or treating
its redb backend as proof that its whole engine is Rust-only.

## Reusable libraries rather than replacement databases

| ID / upstream | License at review | Role and boundary |
| --- | --- | --- |
| `diskann` / [Microsoft DiskANN3](https://github.com/microsoft/DiskANN) | MIT | Current main is a composable Rust vector-index library with a `DataProvider` interface, real-time update algorithms, filters and quantization. B-tree/KV provider examples are directly relevant to an omdb adapter. Older C++ lives on `cpp_main`. No omdb adapter or comparative performance is established here. |
| `hnsw-rs` / [hnsw_rs](https://github.com/jean-pierreBoth/hnswlib-rs) | MIT OR Apache-2.0 | HNSW algorithm/persistence reference, current inspected crate 0.3.6. Distinguish upstream from modified copies embedded in other engines. |
| `rust-cv-hnsw` / [rust-cv/hnsw](https://github.com/rust-cv/hnsw) | MIT | Smaller ANN algorithm candidate with an alloc-oriented dependency configuration. Storage adaptation and target support need verification; inspected latest commit is August 2025. |
| `infinilabs-diskann` / [INFINI Labs DiskANN fork](https://github.com/infinilabs/diskann) | MIT declaration | Older Rust-port lineage, inspected latest commit July 2025. Keep as a fork reference; prioritize current Microsoft DiskANN3 rather than assuming this fork is the current Rust implementation. |
| `tantivy` / [Tantivy](https://github.com/quickwit-oss/tantivy) | MIT | Lexical/BM25, tokenizers, facets, segment machinery and a `Directory` abstraction. A custom omdb directory or generation/outbox bridge still needs implementation. Native compression and mmap vary by feature profile. |
| `datafusion` / [Apache DataFusion](https://github.com/apache/datafusion) | Apache-2.0 | Arrow-based SQL planning and analytical execution with custom table providers. Vectorized execution means columnar query processing, not ANN vector search. Not an OLTP engine or an existing omdb adapter. |
| `petgraph` / [petgraph](https://github.com/petgraph/petgraph) | MIT OR Apache-2.0 | In-process graph algorithms and a possible exact traversal oracle. Does not provide persistence, a graph query language or transactions. |
| `roaring` / [roaring-rs](https://github.com/RoaringBitmap/roaring-rs) | MIT OR Apache-2.0 | Compressed bitmaps for tag/property candidate sets, intersections and unions. Persist stable object-ID mappings and index generations in the authoritative transaction. |
| `differential-dataflow` / [Differential Dataflow](https://github.com/TimelyDataflow/differential-dataflow) | MIT | Incrementally maintain joins, aggregates and recursive graph results. A computation engine, not a durable transaction store; consume committed changes with a replay cursor. |
| `timely-dataflow` / [Timely Dataflow](https://github.com/TimelyDataflow/timely-dataflow) | MIT | Execution/progress substrate for dataflow. Count it as part of the incremental-view design, not as a second database competitor. |
| `openraft` / [OpenRaft](https://github.com/databendlabs/openraft) | MIT OR Apache-2.0 | Optional consensus component when a concrete replicated-authority requirement exists. Requires a storage/state-machine adapter, membership policy, fencing and recovery; does not turn embedded omdb into distributed storage automatically. |

The [DiskANN3 README](https://github.com/microsoft/DiskANN/blob/13bfb8e3911b9897fe68efd601cd656cec357c3f/README.md)
and [workspace manifest](https://github.com/microsoft/DiskANN/blob/13bfb8e3911b9897fe68efd601cd656cec357c3f/Cargo.toml)
are the basis for preferring its current provider API over descriptions of the
legacy C++ engine. Upstream throughput claims were not reproduced in this review.

## Standalone engines and specialized comparators

These can provide reusable code or an independent benchmark. Adopting one as the
authoritative store would be a separate architecture/migration decision.

| ID / upstream | License at review | Capability and limitation |
| --- | --- | --- |
| `grafeo` / [Grafeo](https://github.com/GrafeoDB/grafeo) | Apache-2.0 | Embedded Rust property graph/RDF, multiple graph languages, SQL/PGQ and vector/full-text features. Strong graph comparator. SQL/PGQ is not evidence of all relational SQL semantics. At released 0.6.2 the default embedded build omits engine WAL; explicitly enable WAL and select sync durability for a durable comparison. |
| `helixdb` / [HelixDB](https://github.com/HelixDB/helix-db) | Apache-2.0 | Current graph/vector/full-text engine uses a pinned SlateDB fork, not its older LMDB architecture. SQL is translated into graph operations with a supported subset. Tokio/object-store/native compression increase its footprint. Sampled CI included both green runs and a failing test partition; no all-green claim. |
| `stoolap` / [Stoolap](https://github.com/stoolap/stoolap) | Apache-2.0 | Embedded SQL with joins, CTEs, windows, MVCC/time travel and HNSW. Useful relational/vector alternative, not a native graph query engine. Source default features include an allocator and hosted OS interfaces; a minimal native profile still needs closure review. Default periodic sync is not per-write durability. |
| `turso` / [Turso database engine](https://github.com/tursodatabase/turso) | MIT | Rust SQLite-compatible engine, relational queries and exact vector operations. Inspected roadmap still lists ANN indexing as future work. Not libSQL, not a graph database, and not complete SQLite parity. Feature-dependent SimSIMD/native boundaries remain. |
| `qdrant` / [Qdrant](https://github.com/qdrant/qdrant) | Apache-2.0 | Specialized filtered vector, sparse/hybrid retrieval and distributed service comparator. Not general relational SQL or property-graph storage. Inspected 1.20.0 lockfile has no RocksDB entry; do not repeat obsolete RocksDB descriptions. Native compression/allocator dependencies still need profile review. |
| `lancedb` / [LanceDB](https://github.com/lancedb/lancedb) | Apache-2.0 | Embedded columnar, vector and full-text retrieval. Current Rust local connection returns `NotSupported` for SQL execution and property-graph creation APIs that exist for remote Enterprise connections. Do not advertise those as free embedded features. |
| `samyama` / [Samyama](https://github.com/samyama-ai/samyama-graph) | Apache-2.0 | Active graph/Cypher/HNSW engine. Mandatory RocksDB/C++ in the inspected manifest conflicts with a native Rust-only closure unless replaced or isolated. Not a full relational SQL engine. |
| `agdb` / [agdb](https://github.com/agnesoft/agdb) | Apache-2.0 | Compact native Rust graph store and typed query builder, optional server. Useful graph component/reference; native ANN and a general SQL engine were not established. |
| `oxigraph` / [Oxigraph](https://github.com/oxigraph/oxigraph) | MIT OR Apache-2.0 | RDF/SPARQL and reusable RDF/query components. Not equivalent to a property graph or SQL/ANN database. Default persistent store uses RocksDB/C++; an in-memory build is not a Rust persistent-store replacement. |
| `typedb` / [TypeDB Community](https://github.com/typedb/typedb) | MPL-2.0 at inspected current source | Typed entities, relations, attributes and TypeQL. Current server is Rust but uses RocksDB and its own build/runtime stack. Native ANN was not established. Do not apply old Java/AGPL descriptions to this pin. |
| `cozo` / [Cozo](https://github.com/cozodb/cozo) | MPL-2.0 | Datalog relational/graph/vector fit is attractive. Inspected default head dates to December 2024; current maintenance is a material concern. Default SQLite and optional RocksDB introduce native dependencies. |
| `cozo-community` / [Cozo community fork](https://github.com/cozo-community/cozo) | MPL-2.0 | Fork reference, not evidence that maintenance has resumed. Inspected head also dates to December 2024 and no releases were returned. Keep distinct provenance without counting it as an independently maintained engine. |
| `neumann` / [Neumann](https://github.com/Shadylukin/Neumann) | MIT OR Apache-2.0 | Separate relational/graph/vector engines and a query router over custom storage. Interesting broad design; inspected last commit June 2026 and native zstd dependency mean it is a watchlist candidate, not the default replacement. |
| `indradb` / [IndraDB](https://github.com/indradb/indradb) | MPL-2.0 | Graph library/server reference. Narrower than the requested multimodel surface; inspected latest commit/release August 2025. Dependency closure not fully reviewed. |
| `graphlite` / [GraphLite](https://github.com/GraphLite-AI/GraphLite) | Apache-2.0 | Embedded GQL graph candidate using sled, not redb. Inspected latest commit January 2026. General relational/vector capability and persistence maturity were not established. |

Two source checks particularly affect selection:

- [Helix's current database manifest](https://github.com/HelixDB/helix-db/blob/b524727a283f0197ae64249bad51b80bf607a371/crates/db/Cargo.toml)
  pins `HelixDB/slatedb` at `747f75d851c0105a204ef1b113597e62b752f82b`.
- [LanceDB's connection implementation](https://github.com/lancedb/lancedb/blob/8029428c7152e6b018dcab31f5240e3144caab68/rust/lancedb/src/connection.rs)
  distinguishes local unsupported SQL/graph operations from remote APIs.

## Storage-only alternatives

| ID / upstream | License at review | Role and boundary |
| --- | --- | --- |
| `fjall` / [Fjall](https://github.com/fjall-rs/fjall) | MIT OR Apache-2.0 | Rust embedded LSM/KV and transaction comparison for write-heavy or different storage workloads. No SQL, property-graph or ANN query layer is supplied merely by selecting Fjall. |
| `slatedb` / [SlateDB](https://github.com/slatedb/slatedb) | Apache-2.0 | Embedded object-store-oriented KV/LSM, useful for shared storage and cache/recovery design. Not an omdb extension or a multimodel query engine. Keep the upstream pin distinct from Helix's fork. |
| `surrealkv` / [SurrealKV](https://github.com/surrealdb/surrealkv) | Apache-2.0 | Versioned transactional KV reference. Its license differs from SurrealDB's. Current manifest includes native zstd and compatibility components; this does not add SurrealDB's query features. |

## License-gated references, not source-import selections

These entries preserve the comparison landscape without admitting their code.
Read the exact version's license and component notices before any use outside
the reviewed permission. A future license-conversion date is not today's license.

| ID / upstream | Current reviewed terms | Treatment |
| --- | --- | --- |
| `silk-graph` / [Silk Graph](https://github.com/Kieleth/silk-graph) | FSL-1.0-Apache-2.0; competing-use limitations before conversion | redb-backed operation-log/Merkle-CRDT graph reference. Not currently an unrestricted Apache-2.0 import. |
| `velesdb` / [VelesDB](https://github.com/cyberlife-coder/VelesDB) | Custom VelesDB Core License 1.0 based on ELv2, with competing-product restrictions | Feature/architecture reference only under this selection. Do not label it Apache/MIT or an unrestricted database fork. |
| `surrealdb` / [SurrealDB](https://github.com/surrealdb/surrealdb) | Business Source License 1.1 at reviewed pin | Excluded from adoption under the current selection. Its multimodel breadth does not override the license constraint. |
| `reddb` / [RedDB](https://github.com/reddb-io/reddb) | Business Source License 1.1 | Different project from redb/omdb. Broad multimodel claims retained as reference, not as a permissible replacement selection. |
| `hydradb` / [HydraDB](https://github.com/hydra-db/hydradb) | Elastic License 2.0 at November 10 head | Current pin changed from earlier licensing descriptions. SlateDB graph architecture reference, with C parser and SuiteSparse GraphBLAS requirements; not a Rust-only import. |
| `spacetimedb` / [SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB) | Business Source License 1.1; inspected 2.16.0 additional grant limits production instances and database-service use | Stored-code/transactional-application programming-model reference. Not an unrestricted replicated runtime dependency. Subdirectory terms require separate inspection. |

Primary license references:
[Silk](https://github.com/Kieleth/silk-graph/blob/5c4b7dc4cae74f38c2242cdeb926bda7bd4c02f3/LICENSE.md),
[VelesDB](https://github.com/cyberlife-coder/VelesDB/blob/afc0a4366bb92d0af8872262a2d4752aa2ebb6ee/LICENSE),
[HydraDB](https://github.com/hydra-db/hydradb/blob/af6344037bffb2a354bf32c2b312ba4921a1583f/LICENSE),
[SpacetimeDB](https://github.com/clockworklabs/SpacetimeDB/blob/0b6f11ab88e74df8650af72e9074ef9ced9cfbfa/LICENSE.txt).
License observations are engineering intake information, not a legal clearance
for a combined distribution. MPL components also retain their own obligations.

## Useful extension boundaries

1. Keep canonical rows, object-ID mappings, authoritative edges/tags and committed
   change records in a defined transaction owner. A library's separate commit
   API must be adapted or treated as a derived consumer.
2. Give derived ANN/full-text/graph projections a source frontier, format/model
   identity, generation and rebuild path. Acknowledging an omdb transaction must
   not falsely imply a separate index is already durable or current.
3. Exact tags and current access checks remain authoritative. Approximate search
   may choose candidates; it cannot grant permission, revive deleted records or
   substitute stale results for a requested current snapshot.
4. Select minimal feature profiles. Audit actual target dependencies and native
   boundaries rather than treating a Rust repository or `no_std` flag as proof.
5. Import or adapt selected code with its license, notices and immutable source
   identity. Keep the byte-pinned RuVector tree intact; local integration belongs
   outside that snapshot.

## Maintenance of this catalog

Refresh a candidate when a concrete integration is selected or a material upstream
change affects the decision. Compare commits, release notes, CI and issue
resolution together. Do not rank projects by stars, raw commit volume or a
repository-wide push timestamp. Issue reports are leads until reproduced at the
chosen pin; closed historical reports are not current defects by default.

No engine imports, dependency upgrades, CI schedules, runtime selection or
qualification receipts are created by these references. Existing exact source
and executed evidence keep their original scope.
