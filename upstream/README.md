# Imported RuVector source

`ruvector/` contains every file tracked by
[ruvnet/RuVector](https://github.com/ruvnet/RuVector) at
[`39a104e50388d41837f86e4001096a01927e1728`](https://github.com/ruvnet/RuVector/commit/39a104e50388d41837f86e4001096a01927e1728).
Its Git tree is exactly `4553930c20e6ec1a36c021873c8f86f1ccea2be0`.
No upstream files have been rewritten or removed. This includes Rust crates,
SDKs, patches, examples, documentation, tests, tracked lockfiles, and tracked
binary assets. The snapshot contains 13,884 blobs totaling 389,693,247 bytes.

This imports a source snapshot, not RuVector's entire commit history, release
downloads, or third-party dependencies downloaded by its package managers.
[ruvector-import.json](ruvector-import.json) records the immutable source pins.

## External submodules

RuVector's three submodule gitlinks are preserved unchanged. The repository-root
`.gitmodules` maps them to their new locations; the original nested `.gitmodules`
is also retained as part of the exact source snapshot.

| Subdirectory in `ruvector/` | Source | Pinned commit |
| --- | --- | --- |
| `examples/vectorvroom` | [shaal/VectorVroom](https://github.com/shaal/VectorVroom) | `4c2527b4526ccb8960cd13e3d9e1802d958dca60` |
| `external/ruqu` | [ruvnet/ruqu](https://github.com/ruvnet/ruqu) | `d7f572890a766584d9eed188fa8af5d65c96cca4` |
| `external/rvdna` | [ruvnet/rvdna](https://github.com/ruvnet/rvdna) | `333bc4c40b81adf604452e3b9289f406460b5be1` |

For a complete checkout including these external sources:

```sh
git clone --recurse-submodules https://github.com/glopyglerky/omdb.git
```

For an existing checkout, from the omdb root:

```sh
git submodule update --init --recursive -- upstream/ruvector/examples/vectorvroom upstream/ruvector/external/ruqu upstream/ruvector/external/rvdna
python3 scripts/verify-ruvector-import.py
```

The verifier checks the committed tree and all submodule pins, without compiling
or executing imported source. It does not certify uncommitted working-tree edits.

## Build boundary

The root Cargo workspace, public `redb` API, and on-disk format are unchanged.
The imported workspace is excluded from root workspace builds and from the
published redb package. Upstream workflows remain nested and are not installed
as omdb workflows. No upstream hooks or installers are run by the import.

RuVector already uses redb, but this pinned workspace requests redb `2.1`; omdb's
root package is `redb` `4.5.0`. The import intentionally preserves that upstream
manifest. Connecting selected RuVector crates to the local engine is follow-up
integration work, not an effect of copying their source. This import makes no
claim that all RuVector crates build, share one transaction boundary, or support
omdb's `no_std` targets.

## Attribution and licensing

Keep all upstream copyright, license, NOTICE, and attribution files in place.
RuVector's root [LICENSE](ruvector/LICENSE) declares MIT; individual components
and vendored dependencies retain their own terms. The omdb root license does not
relicense imported material or external submodules. In particular, a submodule
pin is not a license grant: review each component's terms before distributing a
combined product. The pinned VectorVroom tree has no standalone top-level license
file; its gitlink is preserved, not relabeled as omdb-licensed source.

Update the source pin, complete subtree, and import record together when taking
a newer upstream snapshot. Keep local integration code outside this byte-pinned
snapshot so source updates remain auditable.

## Extension and comparison references

The [database extension catalog](../docs/research/database-upstream-catalog.md)
compares RuVector with redb-backed extensions, reusable Rust query/index libraries,
and alternative databases. Its [pinned reference manifest](../docs/research/database-candidates.json)
and [focused comparison plan](../docs/research/database-comparison-plan.md) identify
what to evaluate next without importing more engines or changing the root build.
These are research references, not benchmark results or integrated capabilities.
