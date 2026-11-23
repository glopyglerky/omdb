use std::io::{self, Write};
use std::sync::{Arc, Mutex, OnceLock, Weak};

#[derive(Clone, Copy, Debug)]
pub(crate) enum Phase {
    Preparation,
    VerifyCurrentRoots,
    VerifyAlternateRoots,
    RebuildDataAllocator,
    RebuildSystemAllocator,
    MarkPinnedPages,
    RepairCommit,
    RestoreSavepoints,
    RecordAllocator,
}

impl Phase {
    const ALL: [Self; 9] = [
        Self::Preparation,
        Self::VerifyCurrentRoots,
        Self::VerifyAlternateRoots,
        Self::RebuildDataAllocator,
        Self::RebuildSystemAllocator,
        Self::MarkPinnedPages,
        Self::RepairCommit,
        Self::RestoreSavepoints,
        Self::RecordAllocator,
    ];
}

#[derive(Clone, Copy, Debug, Default)]
struct ReadCounts {
    success: u64,
    failed: u64,
    bytes: u64,
}

#[derive(Debug)]
struct OpenReads {
    phase: Phase,
    counts: [Option<ReadCounts>; Phase::ALL.len()],
}

#[derive(Debug, Default)]
pub(crate) struct Recorder(OnceLock<Weak<Mutex<OpenReads>>>);

impl Recorder {
    pub(crate) fn start(&self) -> Option<Report> {
        if std::env::var_os("REDB_CHECKED_OPEN_DIAGNOSTICS").as_deref() != Some("1".as_ref()) {
            return None;
        }
        let mut counts = [None; Phase::ALL.len()];
        counts[Phase::Preparation as usize] = Some(ReadCounts::default());
        let reads = Arc::new(Mutex::new(OpenReads {
            phase: Phase::Preparation,
            counts,
        }));
        self.0.set(Arc::downgrade(&reads)).ok()?;
        Some(Report(reads))
    }

    fn update(&self, update: impl FnOnce(&mut OpenReads)) {
        if let Some(reads) = self.0.get().and_then(Weak::upgrade)
            && let Ok(mut reads) = reads.lock()
        {
            update(&mut reads);
        }
    }

    pub(crate) fn enter(&self, phase: Phase) {
        self.update(|reads| {
            reads.phase = phase;
            reads.counts[phase as usize].get_or_insert_default();
        });
    }

    pub(crate) fn backend_read(&self, len: usize, success: bool) {
        self.update(|reads| {
            let counts = reads.counts[reads.phase as usize].get_or_insert_default();
            if success {
                counts.success = counts.success.saturating_add(1);
                counts.bytes = counts.bytes.saturating_add(len as u64);
            } else {
                counts.failed = counts.failed.saturating_add(1);
            }
        });
    }
}

pub(crate) struct Report(Arc<Mutex<OpenReads>>);

impl Drop for Report {
    fn drop(&mut self) {
        let Ok(reads) = self.0.lock() else {
            return;
        };
        let counts = reads.counts;
        drop(reads);

        let mut stderr = io::stderr().lock();
        for phase in Phase::ALL {
            if let Some(reads) = counts[phase as usize] {
                let _ = writeln!(
                    stderr,
                    "redb_checked_open phase={phase:?} backend_reads={} backend_read_errors={} backend_bytes={}",
                    reads.success, reads.failed, reads.bytes
                );
            }
        }
    }
}
