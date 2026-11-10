# Rust-first hardware support for omdb

Research date: 2026-11-10. This is a source-backed implementation plan, not a claim that omdb currently boots or controls these devices. No upstream code was imported and no physical hardware was accessed during this research.

## Decision

Build one capability interface with several qualified hardware adapters. Keep hardware discovery, driver binding and access policy inside the product. Users should receive a working capability inventory without choosing a donor operating system or writing board files.

The common interface can cover every resource class. Its implementations will grow by architecture, board, controller and firmware version. A Rust target, a PCI identifier or a successful boot does not establish that all devices on that machine work. Rust itself distinguishes targets that are built from targets that are tested. Upstream hardware projects also publish narrower support matrices. [S1, S2, S3]

The database should hold ownership, grants, desired state, driver identity, operations and outcomes. A small native execution layer must implement boot, allocation, page tables, interrupts, timers, DMA and device access before the database can open. This avoids a circular dependency in which storage initialization needs a database that needs storage initialization.

An adapter should expose typed operations and resource handles. Application records must not confer arbitrary MMIO, physical-memory or PCI configuration access. Translate a checked operation into privileged device activity only after validating owner authority, resource identity, operation scope, deadline and current fencing epoch.

## Upstream inventory

Licenses below are the identities reported by the linked upstream source. Each import still needs the exact revision, file notices, dependency licenses and redistribution terms recorded. A project-wide label does not license vendor firmware or every bundled dependency.

| Resource or function | Upstream reuse route | Verified scope and limit | License reported by source |
| --- | --- | --- | --- |
| Firmware handoff | `rust-osdev/uefi-rs` | Rust UEFI interfaces for platforms supported by Rust and UEFI. Its tests cover a finite set of architectures and firmware combinations. After boot-services exit, only the specified runtime services remain available. [S4, S30] | MIT OR Apache-2.0 |
| First PC boot | `rust-osdev/bootloader` | Experimental x86_64 BIOS and UEFI loader, with a documented `x86_64-unknown-none` kernel path. It is a useful first boot target, not a cross-architecture loader. [S5] | MIT OR Apache-2.0 |
| Platform composition | ArceOS and its platform packages | Modular Rust OS with x86_64, aarch64, riscv64 and loongarch64 examples; QEMU machine targets and selected real-device examples. The README describes platform selection and driver feature selection. omdb packaging can internalize those choices. [S2] | Repository contains Apache-2.0, GPL-3.0 and Mulan license alternatives; inspect the selected crate manifest |
| ACPI discovery and power descriptions | `rust-osdev/acpi` | Static-table parsing and AML interpretation. The no-allocation interface has a smaller scope; richer parsing requires allocation. Firmware tables can be nonconforming. This identifies and describes resources but does not implement every device. [S6] | MIT OR Apache-2.0 |
| Device-tree discovery | `repnop/fdt` | Pure Rust `no_std` flattened-device-tree parser. Board-specific clock, reset, interrupt and power behavior still needs a driver implementation. [S7] | MPL-2.0 |
| PCI and PCIe discovery | `rust-osdev/pci_types` | Types for PCI enumeration and configuration. Enumeration alone does not give safe DMA, reset, interrupt delivery or a functional device driver. [S8] | MIT OR Apache-2.0 |
| x86 memory and processor control | `rust-osdev/x86_64` | Architecture-specific instructions, registers and page-table structures. Equivalent verified implementations are required for the other architectures. [S9] | MIT OR Apache-2.0 |
| First block, network and console devices | `rcore-os/virtio-drivers` | `no_std` drivers for VirtIO devices and MMIO/PCI transports. Its `Hal` implementation must supply DMA allocation and address translation. The support table explicitly lists unavailable transport features. [S10, S11] | MIT |
| Real storage and network controllers | Redox drivers | Candidate NVMe, AHCI, xHCI, Ethernet and other drivers. They are designed as Redox user-space daemons using its system calls and schemes. Importing device logic requires replacing those dependencies. The GitHub mirror is archived and points to the maintained GitLab source. [S12] | MIT in the inspected driver repository |
| Network stack | `smoltcp` | Rust stack for bare-metal and real-time systems. It supplies network protocol handling, not the NIC driver, TLS, a complete QUIC runtime or outbound relay service. [S13] | 0BSD |
| Embedded buses and sensors | `embedded-hal` and device drivers built on it | Reusable blocking, async and polling interfaces for supported bus and I/O operations. The scope is embedded peripheral access, not a universal PCI/GPU abstraction. [S14] | MIT OR Apache-2.0 |
| Microcontroller execution | Embassy | Async embedded framework with chip-family HALs and device examples. Select and qualify the exact chip implementation. A chip-family name does not imply every peripheral or wireless feature works. [S15] | MIT OR Apache-2.0 |
| Independently reliable management controller | Hubris | Memory-protected message-passing environment for deeply embedded systems, with board-specific application images and driver modules. Useful for thermal, power and watchdog controllers that must survive the main database's failure. [S16] | MPL-2.0 |
| Isolated embedded applications | Tock | Rust embedded OS with explicit board support tiers. Its board documentation separates regularly tested boards from experimental ports. [S17] | MIT OR Apache-2.0 |
| Restartable Rust module research | Theseus | Research OS for language-based resource management, fault recovery and live evolution. Its upstream describes limited tested real machines and ongoing maturity work. Reuse specific techniques and qualified modules rather than treating it as universal live-update support. [S18] | MIT |
| Simple display and HMI | `embedded-graphics` | Allocation-free `no_std` 2D drawing with a `DrawTarget` interface. It deliberately leaves display drivers to other crates. Good for a console, status display and embedded controls; not a full desktop compositor. [S19] | MIT OR Apache-2.0 |
| Rich embedded UI | Slint MCU backend | Documented bare-metal MCU path requiring appropriate HALs, display support and a global allocator. Its current package license has GPL and Slint-specific alternatives, so choose terms explicitly before adoption. [S20, S21] | GPL-3.0-only OR Slint license alternatives |
| RGB device knowledge | OpenRGB | Concrete device and protocol coverage over USB, SMBus and other paths. Its support list distinguishes partial, problematic and unsupported devices. The implementation is an isolated compatibility candidate or a licensed source for a device-specific port. [S22, S23] | GPL-2.0 |
| Cooling and power-controller knowledge | liquidctl | Python drivers and device-specific notes for cooling, fan, LED and power devices. Its matrix marks partial and broken support. Useful protocol evidence and compatibility implementation, not native Rust support. [S24] | GPL-3.0 |
| Missing Linux-only drivers | Linux and Rust-for-Linux, isolated by device domain | Rust kernel abstractions explicitly wrap C kernel functionality. A Rust Linux driver cannot generally be copied into a freestanding kernel unchanged. Keep necessary Linux execution and licenses visible in that adapter. [S25] | Audit exact Linux driver and dependencies |
| Apple Silicon hardware knowledge | Asahi platform documentation and drivers | Per-SoC and per-device feature tables show support for display, GPU, NVMe, power, audio and other blocks, including incomplete functions. These are Linux-platform integrations, not a standalone universal Rust HAL. [S26, S27] | Audit exact source, Mesa/kernel split and firmware separately |
| Emerging NVIDIA Rust driver | Linux Nova | Upstream separates a GSP hardware/firmware core from second-level drivers. Follow its actual task status. It is evidence of a reuse path, not proof of production graphics or compute parity for every GPU. [S28] | Audit exact Linux files and firmware |

Redox's own hardware report currently lists restrictions on ACPI, Wi-Fi, Bluetooth, I2C, input and GPU coverage. That is a useful warning against equating a substantial Rust OS with universal device support. [S3]

## A single interface with explicit capability evidence

The following fields are a proposed contract, not existing omdb behavior.

Each discovered resource needs a stable node-local identity, physical owner, topology location, device identifiers, firmware revision, reset group, failure domain and current generation. Each offered operation needs an implementation revision, supported range, isolation requirements, health, measured limits, evidence and a status such as detected, supported, qualified, degraded or unavailable.

Keep intrinsic resource properties separate from policy. Storage capacity, persistence, write endurance and flush behavior describe a medium. Its owner, lease, sharing scope and allowed operations govern use. RAM and block devices can share the latter model without pretending they have the same persistence or addressing behavior.

Discovery can combine firmware tables, device tree, PCI enumeration, USB descriptors and management interfaces. Driver selection should match qualified device rules and actual required capabilities, with deterministic selection and an auditable reason. A match by vendor/product ID is only a candidate. Driver initialization and conformance checks still have to succeed.

Use three implementation routes behind that interface:

1. A native Rust driver where the controller and required operations are qualified.
2. A qualified non-Rust or OS-dependent driver in a confined device domain where native Rust coverage is missing.
3. An explicit unavailable capability, with the missing implementation or hardware prerequisite recorded.

The second route needs an actual isolation implementation. A process boundary alone does not confine device DMA; neither does a Rust wrapper or trait object. Linux VFIO is a reference for IOMMU-backed device assignment and group-level ownership. Its documentation explains that devices in an IOMMU group cannot have competing DMA owners. Native omdb must implement and verify equivalent ownership requirements for its own memory and interrupt model. [S29]

The safety and correctness contract needs to cover DMA mappings, address-space ownership, cache coherency, barriers, interrupt routing, outstanding commands, timeouts, cancellation, quiescence, device reset and hot removal. Revoking a database grant is insufficient while an old DMA queue can still write memory.

## What belongs outside a database write transaction

A transaction can authorize a fan target, reserve a storage extent, record a requested display mode or commit a compute job. The driver performs the operation and records its observed outcome with the same operation identity and current epoch.

Interrupt handling, DMA completion, network progress, display scanout, watchdogs and emergency thermal control must not wait for a database writer, a cluster quorum or a database repair. The runtime has to keep them functioning while the database is slow or unavailable. Hardware-enforced limits and a management controller provide a stronger failure path than a task queued behind application writes.

An external side effect is not automatically atomic with the database. Use explicit operation states, idempotency keys where the device supports them, reconciliation and reset semantics. Report an unknown outcome when a crash makes it unknowable. Replaying a device command can be unsafe even when replaying a database transaction is safe.

RGB and display frames also should not require a durable transaction for every hardware update. Persist configurations and desired behavior, then let a bounded driver stream frames or effects. Retain sampled observations and failures according to a policy that avoids needless SSD writes.

## First end-to-end boot slice

Use a controlled QEMU machine to prove the storage and execution contracts before expanding device coverage. The preferred first route is an ArceOS composition with VirtIO block, network and console because it already has reusable platform and driver modules. A minimal x86_64 bootloader plus omdb-specific runtime is an alternative if that integration proves smaller. Choose one after an actual integration spike. Both are proposals, not completed boot evidence. [S2, S5, S10]

The first acceptance sequence should be:

1. Build a pinned freestanding image with the selected boot path, allocator, timer, panic path and bounded synchronization implementation.
2. Boot in QEMU, enumerate the expected memory and device resources, and publish the actual capability inventory over a serial console.
3. Open omdb through a raw VirtIO block-backed `StorageBackend`. Reserve an explicit owned extent and reject out-of-range I/O. Do not write to a host physical disk.
4. Commit a state transition, operation receipt and durable continuation. The storage adapter must implement the exact omdb synchronization contract using verified device flush semantics.
5. Kill and restart the emulator at controlled points around persistence. Check recovery, receipt identity and absence of acknowledged data loss under the declared failure model.
6. Exercise grant expiry, epoch change, driver restart and hot removal. A command from a stale owner must not reach the reallocated resource.
7. Add VirtIO network and an authenticated outbound session. Network success must not quietly change the local durability promise into a global one.
8. Add a framebuffer or VirtIO display, input and a small status HMI. Rendering must remain responsive under writer contention.
9. Repeat the same storage/authority tests on a second architecture before claiming portability of the integration. Qualify actual machines separately from QEMU.

The first nonvolatile acknowledgment contract should state its failure domain explicitly. A successful memory write or replicated RAM copy does not establish survival across power loss. A device's advertised flush capability also needs verification in the chosen backend and emulator configuration.

After this slice, expand controller families in order of the product's required functions. Keep tests for boot, shutdown, sustained I/O, thermal behavior, suspend/resume when supported, reset and recovery attached to each qualified platform. The same database schema can describe every machine while the evidence makes clear which operations actually work.

## Import and maintenance rules

Pin donor revisions and keep source URLs, licenses, notices, local changes and acceptance results with each imported implementation. Track upstream updates against those exact revisions. Do not merge several complete kernels merely to collect their driver directories. Their allocators, concurrency models, address spaces and boot assumptions conflict unless deliberately adapted.

Keep the native interface stable as a driver changes. Run the same conformance tests against native and compatibility adapters. A compatibility adapter can retire only after the replacement supports the actual required operations on the target devices and passes the recovery tests.

Firmware and microcode remain separate dependencies even where all host-side driver code is Rust. Record their versions and redistribution requirements rather than describing that machine as entirely Rust.

## Sources

- S1: [Rust target support policy and target tables](https://doc.rust-lang.org/rustc/platform-support.html).
- S2: [ArceOS README and platform examples](https://github.com/arceos-org/arceos), [module architecture](https://github.com/arceos-org/arceos/blob/main/doc/README.md).
- S3: [Redox hardware compatibility report](https://github.com/redox-os/redox/blob/master/HARDWARE.md).
- S4: [uefi-rs README](https://github.com/rust-osdev/uefi-rs), [supported compiler and architecture documentation](https://github.com/rust-osdev/uefi-rs/blob/main/uefi/src/lib.rs).
- S5: [rust-osdev bootloader README](https://github.com/rust-osdev/bootloader/blob/main/README.md).
- S6: [acpi README](https://github.com/rust-osdev/acpi).
- S7: [fdt source and license](https://docs.rs/fdt/latest/src/fdt/lib.rs.html).
- S8: [pci_types README](https://github.com/rust-osdev/pci_types).
- S9: [x86_64 crate](https://github.com/rust-osdev/x86_64), [package license](https://docs.rs/crate/x86_64/latest/source/Cargo.toml).
- S10: [virtio-drivers support table](https://github.com/rcore-os/virtio-drivers).
- S11: [virtio-drivers HAL requirements](https://docs.rs/virtio-drivers/latest/virtio_drivers/).
- S12: [Redox drivers, design and maintenance location](https://github.com/redox-os/drivers), [driver license](https://github.com/redox-os/drivers/blob/master/LICENSE).
- S13: [smoltcp](https://github.com/smoltcp-rs/smoltcp).
- S14: [embedded-hal scope and crate family](https://github.com/rust-embedded/embedded-hal).
- S15: [Embassy](https://github.com/embassy-rs/embassy).
- S16: [Hubris README](https://github.com/oxidecomputer/hubris/blob/master/README.mkdn), [repository and license](https://github.com/oxidecomputer/hubris).
- S17: [Tock](https://github.com/tock/tock), [board qualification tiers](https://github.com/tock/tock/blob/master/boards/README.md).
- S18: [Theseus](https://github.com/theseus-os/Theseus), [tested real-machine scope](https://github.com/theseus-os/Theseus/blob/theseus_main/README.md).
- S19: [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics).
- S20: [Slint bare-metal MCU documentation](https://docs.slint.dev/latest/docs/rust/slint/docs/mcu/).
- S21: [Slint package license declaration](https://github.com/slint-ui/slint/blob/master/Cargo.toml).
- S22: [OpenRGB device-support statuses](https://openrgb.org/devices_pipeline.html).
- S23: [OpenRGB source and license](https://github.com/CalcProgrammer1/OpenRGB), [SMBus integration documentation](https://gitlab.com/CalcProgrammer1/OpenRGB/-/blob/24fc6bd749bc70380eb52c63cd9dbe83deb3ea89/Documentation/SMBusAccess.md).
- S24: [liquidctl device matrix](https://github.com/liquidctl/liquidctl/blob/main/README.md), [source and license](https://github.com/liquidctl/liquidctl).
- S25: [Linux Rust abstractions and C bindings](https://www.kernel.org/doc/html/latest/rust/general-information.html).
- S26: [Asahi M1 feature support](https://asahilinux.org/docs/platform/feature-support/m1/).
- S27: [Asahi M2 feature support](https://asahilinux.org/docs/platform/feature-support/m2/).
- S28: [Linux Nova driver architecture](https://kernel.org/doc/html/next/gpu/nova/index.html).
- S29: [Linux VFIO and IOMMU ownership](https://kernel.org/doc/html/latest/driver-api/vfio.html).
- S30: [UEFI boot services and runtime services](https://uefi.org/specs/UEFI/2.11/08_Services_Runtime_Services.html).
