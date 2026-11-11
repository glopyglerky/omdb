# Focused database extension comparisons

Status: proposed comparison references, not implemented tests or measured results.
Reviewed: 2026-11-10, America/Denver.

Use [the catalog](database-upstream-catalog.md) and
[exact candidate pins](database-candidates.json). Start with the capability being
added and one baseline plus one challenger. Do not install all catalog entries,
rerun redb's whole qualification, or add another mandatory CI matrix for research.
Reuse relevant upstream tests where possible and add only the adapter-boundary
cases they do not cover.

## Initial comparison pairs

| Need | Baseline | First challenger or reference | Cases |
| --- | --- | --- | --- |
| ANN over omdb | Imported RuVector core | DiskANN3 provider, then shodh-redb IVF-PQ if justified | `vector-filtered`, `mutation-reopen`, `transaction-boundary`, `profile-closure` |
| SQL over omdb | Existing exact table operations | GlueSQL adapter; ContextDB shared commit as design reference | `sql-semantics`, `transaction-boundary`, `mutation-reopen`, `profile-closure` |
| Exact tags | Table/multimap postings | roaring-rs candidate bitmaps | `tag-filter`, `transaction-boundary`, `profile-closure` |
| Graph traversal/query | Exact adjacency traversal / RuVector graph | ContextDB or Grafeo; NopalDB for temporal graph requirements | `graph-semantics`, `transaction-boundary`, `mutation-reopen`, `profile-closure` |
| Lexical/hybrid search | Exact token/property baseline | Tantivy; Corvid as integrated reference | `text-hybrid`, `mutation-reopen`, `transaction-boundary` |
| Separate SQL/vector engine | Current omdb application boundary | Stoolap, with equal durability | `sql-semantics`, `vector-filtered`, `mutation-reopen`, `profile-closure` |
| Incremental views | Recompute the small exact query | Differential Dataflow / Timely | `incremental-view`, `transaction-boundary`, `profile-closure` |

Qdrant and LanceDB are optional independent retrieval comparators, not mandatory
services. Storage and replication comparisons remain separate unless the selected
operation actually needs a new storage engine or replicated authority.

## Shared case definitions

Each case is a contract to apply to a suitable candidate, not a claim that every
candidate implements it. Unsupported behavior is `not_applicable` or
`unsupported`, never a pass and never emulated silently by a different engine.

| Case ID | Minimal useful check | Result to retain |
| --- | --- | --- |
| `vector-filtered` | Fixed synthetic vectors and exact distance oracle; top-k with no filter and selective tag/owner filters; updates, ties, duplicate IDs, invalid dimensions and non-finite values. Distinguish true filtered search from post-filtering an unfiltered top-k. | Recall against exact eligible ground truth, result eligibility, requested/returned count, metric/normalization and index parameters. |
| `graph-semantics` | Directed typed edges, properties, cycles, disconnected nodes, depth bounds, duplicate-edge policy, deletion and reverse traversal; compare exact reachable IDs and paths. | Supported language subset, exact set/path agreement, edge/node version and bounded-work behavior. |
| `sql-semantics` | Inner/left joins, nulls, numeric comparison, grouping and a small recursive query only when supported; rollback and conflicting updates at the actual adapter boundary. | Exact rows and errors; dialect/transaction differences recorded explicitly. A SQL-looking API is not proof of SQL parity. |
| `tag-filter` | Exact AND/OR/NOT and range/owner constraints over stable IDs; remove/update a tag and compare postings or bitmaps with an exact scan. | Set equality, stale-ID rejection, mutation cost and mapping/storage overhead. A tag is not itself an authorization grant. |
| `text-hybrid` | Fixed small corpus with repeated terms, Unicode, field filters, updates/deletes and an empty lexical result; compare lexical and hybrid outputs under the same eligible set. | Tokenizer and scoring settings, candidate coverage, deterministic tie policy and index frontier. Score values across engines need not match. |
| `transaction-boundary` | One logical operation changes a row, edge/tag and vector record. Abort before commit and reopen after commit. If indexes are separate, interrupt between authoritative commit and index publication and replay exactly once or idempotently. | Explicit single-transaction support or derived-index/outbox contract, visible frontier, no phantom partial authoritative success. Use a disposable store, not production data. |
| `mutation-reopen` | Insert, update, delete, close and reopen one disposable database; verify exact stored payloads and membership. Add one abrupt process-stop boundary only when adapting persistence or commit ownership. | Durability mode, acknowledged frontier, recovery/rebuild requirement and exact content identity. Clean close does not establish power-loss durability. |
| `profile-closure` | Resolve the selected lock/features/target, inspect native libraries and unsafe boundaries, and compile only the proposed adapter/profile when implementation begins. | Compiler/target/lock identities, required OS services, binary/RSS where measured, and actual no_std status. No performance claim from cross-compilation alone. |
| `incremental-view` | Feed a bounded committed insert/update/delete stream, replay a duplicate batch, resume at a saved frontier, and compare the incremental join/aggregate with full recomputation. | Exact outputs, replay semantics, lag/frontier and bounded memory; derived results do not become authority. |
| `replication-boundary` | Only for a selected replication requirement: interrupt a disposable leader, reject a stale writer, restore a snapshot and reconcile an uncertain write under the declared quorum. | Storage/state-machine ownership, fencing, membership assumptions and acknowledged durability. Do not run this for an ordinary index import. |
| `storage-workload` | Only for a measured storage problem: compare equivalent reads/writes/ranges with identical durability, payloads and cache conditions through the existing storage comparison harness. | Useful-operation latency, allocation/write amplification, recovery and resource cost. KV wins do not establish multimodel query wins. |

## Keep the first pass small

Start with a fixed fixture of about 1,000 records, a small cyclic graph and at
most 100 selected queries. Use deterministic input and immutable source pins.
This is enough to expose adapter, filter, deletion and commit-boundary errors;
it is not a production-scale recall, lifetime or durability qualification.

The exact vector oracle defines results using the same metric and normalization
as the candidate, with an explicit numerical tolerance and tie rule. Track
selectivity separately from recall. Zero eligible records and fewer than k
eligible records have defined, non-error results. Security eligibility is exact
even when ranking is approximate.

Only candidates that implement the required semantics proceed to an optional
workload-sized measurement. Use the same data, durable-commit settings, threads,
memory limit and cache state. Include build time, index size, update/delete cost,
reopen/rebuild and p50/p95 query latency. Record ANN recall alongside latency;
do not compare unlike recall targets or periodic fsync against per-write fsync.

## Reuse existing harnesses

- Preserve upstream tests and fixtures in the pinned RuVector snapshot. Run only
  the selected crates/profile when that adapter is changed.
- Use DiskANN3's existing `diskann-benchmark` and provider examples for vector
  inputs and algorithm comparisons. They do not already test an omdb adapter.
- Reuse GlueSQL's redb-storage and SQL tests for its supported dialect. Add a
  caller-transaction boundary case if that integration is implemented.
- Reuse the chosen graph engine's query and persistence suites for its declared
  language; add only the integration-specific object/authority boundary.
- Put any future heavyweight storage-engine comparisons in
  `crates/redb-bench-compare`, as required by the root contributor instructions,
  rather than expanding normal `redb-bench` dependencies.

No executable harness, dependency download, benchmark data, external service or
schedule is added by this document. The JSON manifest is reference data, not a
runner and not permission to execute upstream installers or tests.

## Result record

When a comparison actually runs, retain candidate and baseline commit IDs,
adapter source, lockfile and selected features, compiler/target, fixture digest,
configuration, command, applicable cases, actual results and limits. Use the
states `passed`, `failed`, `unsupported`, `not_applicable` or `not_run` separately.
At catalog creation all comparison results are `not_run`.

Choose an implementation for a stated workload and maintenance cost. Keep a
working exact fallback while approximate or experimental behavior remains
unqualified. A catalog entry, successful upstream CI run or source copy does not
qualify an omdb integration.
