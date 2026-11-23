use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use fjall_contract::Readable;
use rusqlite::{Connection, TransactionBehavior, params};

type Row<'a> = (&'a [u8], &'a [u8]);
type Rows = Vec<(Vec<u8>, Vec<u8>)>;

trait Snapshot {
    fn range(&self, start: &[u8], end: &[u8]) -> Rows;
}

trait Store: Sized {
    type Reader: Snapshot;

    fn open(path: &Path) -> Self;
    fn read(&self) -> Self::Reader;
    fn mutate_before_commit(
        &self,
        rows: &[Row<'_>],
        removed: &[&[u8]],
        commit: bool,
        staged: impl FnOnce(),
    );

    fn write_before_commit(&self, rows: &[Row<'_>], commit: bool, staged: impl FnOnce()) {
        self.mutate_before_commit(rows, &[], commit, staged);
    }

    fn write(&self, rows: &[Row<'_>], commit: bool) {
        self.write_before_commit(rows, commit, || {});
    }
}

fn exercise<S: Store>() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let store = S::open(&path);
    let before: &[Row<'_>] = &[
        (b"1:event", b"old event"),
        (b"2:projection", b"old projection"),
        (b"3:index", b"old index"),
    ];
    let after: &[Row<'_>] = &[
        (b"1:event", b"new event"),
        (b"2:projection", b"new projection"),
        (b"3:index", b"new index"),
        (b"4:receipt", b"accepted"),
        (b"5:outbox", b"pending"),
    ];
    let expected = |rows: &[Row<'_>]| -> Rows {
        rows.iter()
            .map(|&(key, value)| (key.to_vec(), value.to_vec()))
            .collect()
    };
    let window = |snapshot: &S::Reader| snapshot.range(b"1:", b"6:");

    store.write(&[(b"0:outside", b"lower"), (b"6:outside", b"upper")], true);
    store.write(before, true);
    let pinned = store.read();
    assert_eq!(window(&pinned), expected(before));
    store.write(after, false);
    assert_eq!(window(&store.read()), expected(before));

    store.write(after, true);
    assert_eq!(window(&pinned), expected(before));
    assert_eq!(window(&store.read()), expected(after));
    drop(pinned);
    drop(store);

    let reopened = S::open(&path);
    assert_eq!(window(&reopened.read()), expected(after));
}

macro_rules! redb_store {
    ($module:ident, $engine:ident) => {
        mod $module {
            use super::{Path, Row, Rows, Snapshot, Store};
            use $engine::{ReadableDatabase, ReadableTable};

            const TABLE: $engine::TableDefinition<&[u8], &[u8]> =
                $engine::TableDefinition::new("ordered");

            pub(super) struct Database($engine::Database);

            impl Store for Database {
                type Reader = $engine::ReadTransaction;

                fn open(path: &Path) -> Self {
                    let database = $engine::Database::create(path).unwrap();
                    let mut txn = database.begin_write().unwrap();
                    txn.set_durability($engine::Durability::Immediate).unwrap();
                    txn.open_table(TABLE).unwrap();
                    txn.commit().unwrap();
                    Self(database)
                }

                fn read(&self) -> Self::Reader {
                    self.0.begin_read().unwrap()
                }

                fn mutate_before_commit(
                    &self,
                    rows: &[Row<'_>],
                    removed: &[&[u8]],
                    commit: bool,
                    staged: impl FnOnce(),
                ) {
                    let mut txn = self.0.begin_write().unwrap();
                    txn.set_durability($engine::Durability::Immediate).unwrap();
                    {
                        let mut table = txn.open_table(TABLE).unwrap();
                        for &(key, value) in rows {
                            table.insert(key, value).unwrap();
                        }
                        for &key in removed {
                            table.remove(key).unwrap();
                        }
                    }
                    staged();
                    if commit {
                        txn.commit().unwrap();
                    }
                }
            }

            impl Snapshot for $engine::ReadTransaction {
                fn range(&self, start: &[u8], end: &[u8]) -> Rows {
                    self.open_table(TABLE)
                        .unwrap()
                        .range(start..end)
                        .unwrap()
                        .map(|row| {
                            let (key, value) = row.unwrap();
                            (key.value().to_vec(), value.value().to_vec())
                        })
                        .collect()
                }
            }
        }
    };
}

redb_store!(local, redb);
redb_store!(direct, redb_direct);

struct Fjall {
    db: fjall_contract::SingleWriterTxDatabase,
    keys: fjall_contract::SingleWriterTxKeyspace,
}

struct FjallRead {
    read: fjall_contract::Snapshot,
    keys: fjall_contract::SingleWriterTxKeyspace,
}

impl Store for Fjall {
    type Reader = FjallRead;

    fn open(path: &Path) -> Self {
        let db = fjall_contract::SingleWriterTxDatabase::builder(path)
            .open()
            .unwrap();
        let keys = db.keyspace("ordered", Default::default).unwrap();
        Self { db, keys }
    }

    fn read(&self) -> Self::Reader {
        FjallRead {
            read: self.db.read_tx(),
            keys: self.keys.clone(),
        }
    }

    fn mutate_before_commit(
        &self,
        rows: &[Row<'_>],
        removed: &[&[u8]],
        commit: bool,
        staged: impl FnOnce(),
    ) {
        let mut txn = self
            .db
            .write_tx()
            .durability(Some(fjall_contract::PersistMode::SyncAll));
        for &(key, value) in rows {
            txn.insert(&self.keys, key, value);
        }
        for &key in removed {
            txn.remove(&self.keys, key);
        }
        staged();
        if commit {
            txn.commit().unwrap();
        }
    }
}

impl Snapshot for FjallRead {
    fn range(&self, start: &[u8], end: &[u8]) -> Rows {
        self.read
            .range(&self.keys, start..end)
            .map(|row| {
                let (key, value) = row.into_inner().unwrap();
                (key.to_vec(), value.to_vec())
            })
            .collect()
    }
}

struct Sqlite {
    path: PathBuf,
    writer: Mutex<Connection>,
}

impl Sqlite {
    fn connect(path: &Path) -> Connection {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch("PRAGMA synchronous=FULL;").unwrap();
        conn
    }
}

impl Store for Sqlite {
    type Reader = Connection;

    fn open(path: &Path) -> Self {
        let writer = Self::connect(path);
        writer
            .execute_batch(
                "PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS ordered
             (key BLOB PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;",
            )
            .unwrap();
        Self {
            path: path.to_owned(),
            writer: Mutex::new(writer),
        }
    }

    fn read(&self) -> Self::Reader {
        let conn = Self::connect(&self.path);
        conn.execute_batch("BEGIN DEFERRED;").unwrap();
        conn
    }

    fn mutate_before_commit(
        &self,
        rows: &[Row<'_>],
        removed: &[&[u8]],
        commit: bool,
        staged: impl FnOnce(),
    ) {
        let mut conn = self.writer.lock().unwrap();
        let txn = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        {
            let mut upsert = txn
                .prepare_cached("INSERT OR REPLACE INTO ordered VALUES (?, ?)")
                .unwrap();
            for &(key, value) in rows {
                upsert.execute(params![key, value]).unwrap();
            }
            if !removed.is_empty() {
                let mut delete = txn
                    .prepare_cached("DELETE FROM ordered WHERE key = ?")
                    .unwrap();
                for &key in removed {
                    delete.execute(params![key]).unwrap();
                }
            }
        }
        staged();
        if commit {
            txn.commit().unwrap();
        }
    }
}

impl Snapshot for Connection {
    fn range(&self, start: &[u8], end: &[u8]) -> Rows {
        let mut stmt = self
            .prepare_cached(
                "SELECT key, value FROM ordered WHERE key >= ? AND key < ? ORDER BY key",
            )
            .unwrap();
        stmt.query_map(params![start, end], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }
}

#[test]
fn omdb_ordered_snapshot_and_root_commit() {
    exercise::<local::Database>();
}

#[test]
fn direct_redb_ordered_snapshot_and_root_commit() {
    exercise::<direct::Database>();
}

#[test]
fn fjall_ordered_snapshot_and_root_commit() {
    exercise::<Fjall>();
}

#[test]
fn sqlite_wal_full_ordered_snapshot_and_root_commit() {
    exercise::<Sqlite>();
}

#[cfg(feature = "ordered-transactions")]
#[test]
fn comparable_transactions() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use redb_bench_compare::ordered::{Database, Engine, Error};
    use std::io::{Error as IoError, ErrorKind};

    type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
    let record = |key: &[u8], value: &[u8]| (key.to_vec(), value.to_vec());

    for engine in [Engine::Fjall, Engine::DirectRedb] {
        let workspace = tempfile::tempdir()?;
        let path = workspace.path().join("state");
        let store = Database::open(engine, &path)?;
        store.write(|txn| {
            txn.put(b"a", b"before")?;
            txn.put(b"a\0", b"prefix")?;
            txn.put(b"b", b"old")?;
            txn.put(b"c", b"exclusive end")
        })?;
        let pinned = store.snapshot()?;

        store.write(|txn| -> TestResult<()> {
            txn.delete(b"a")?;
            txn.put(b"b", b"new")?;
            assert_eq!(txn.get(b"a")?, None);
            assert_eq!(txn.get(b"b")?, Some(b"new".to_vec()));

            let rejected = txn.savepoint(|child| -> TestResult<()> {
                child.delete(b"b")?;
                child.savepoint(|grandchild| grandchild.put(b"a\x01", b"rolled back"))?;
                assert_eq!(
                    child.scan(b"a", b"c", 2)?,
                    [record(b"a\0", b"prefix"), record(b"a\x01", b"rolled back")]
                );
                Err(IoError::new(ErrorKind::Interrupted, "reject child").into())
            });
            assert_eq!(
                rejected.unwrap_err().downcast::<IoError>().unwrap().kind(),
                ErrorKind::Interrupted
            );
            assert_eq!(txn.get(b"a\x01")?, None);
            txn.savepoint(|child| child.put(b"a\x02", b"kept sibling"))?;
            assert_eq!(
                txn.scan(b"a", b"c", 2)?,
                [record(b"a\0", b"prefix"), record(b"a\x02", b"kept sibling")]
            );
            assert_eq!(txn.scan(b"b", b"c", 2)?, [record(b"b", b"new")]);

            let foreground = store.snapshot()?;
            assert_eq!(foreground.get(b"b")?, Some(b"old".to_vec()));
            assert_eq!(foreground.get(b"a\x02")?, None);
            Ok(())
        })?;
        assert_eq!(pinned.get(b"a")?, Some(b"before".to_vec()));
        assert_eq!(pinned.get(b"b")?, Some(b"old".to_vec()));

        let aborted = store.write(|txn| -> Result<(), Error> {
            txn.put(b"b", b"never published")?;
            txn.put(b"", b"invalid key")
        });
        assert!(matches!(aborted, Err(Error::InvalidKey)));
        let fresh = store.snapshot()?;
        assert_eq!(fresh.get(b"a")?, None);
        assert_eq!(fresh.get(b"b")?, Some(b"new".to_vec()));
        assert_eq!(fresh.get(b"a\x02")?, Some(b"kept sibling".to_vec()));
        assert_eq!(fresh.get(b"c")?, Some(b"exclusive end".to_vec()));
        for (start, end, limit) in [
            (b"a".as_slice(), b"c".as_slice(), 0),
            (b"c".as_slice(), b"a".as_slice(), 2),
            (b"a".as_slice(), b"a".as_slice(), 2),
            (b"a".as_slice(), b"c".as_slice(), usize::MAX),
        ] {
            assert!(matches!(
                fresh.scan(start, end, limit),
                Err(Error::InvalidPage)
            ));
        }
        let body = vec![0x42; 9 * 1024 * 1024];
        assert!(matches!(
            store.write(|txn| {
                txn.put(b"z1", &body)?;
                txn.put(b"z2", &body)
            }),
            Err(Error::BudgetExceeded)
        ));
        assert_eq!(store.snapshot()?.get(b"z1")?, None);
        store.write(|txn| txn.put(b"z1", &body))?;
        store.write(|txn| txn.put(b"z2", &body))?;
        let large = store.snapshot()?;
        assert_eq!(large.scan(b"z1", b"z3", 1)?[0].1, body);
        assert!(matches!(
            large.scan(b"z1", b"z3", 2),
            Err(Error::BudgetExceeded)
        ));

        drop((pinned, fresh, large, store));
        let reopened = Database::open(engine, &path)?;
        assert_eq!(
            reopened.snapshot()?.scan(b"a", b"c", 3)?,
            [
                record(b"a\0", b"prefix"),
                record(b"a\x02", b"kept sibling"),
                record(b"b", b"new"),
            ]
        );
        let file = workspace.path().join("not-a-directory");
        std::fs::write(&file, b"leave intact")?;
        assert!(matches!(
            Database::open(engine, &file.join("state")),
            Err(Error::Precommit(_))
        ));
        assert_eq!(std::fs::read(&file)?, b"leave intact");
    }
    Ok(())
}

const HISTORY_VALUE: [u8; 1_024] = [0x35; 1_024];

fn seed_history<S: Store>(store: &S) {
    for start in (0..8_192u32).step_by(256) {
        let keys = (start..start + 256)
            .map(|id| format!("history:{id:06}"))
            .collect::<Vec<_>>();
        let rows = keys
            .iter()
            .map(|key| (key.as_bytes(), HISTORY_VALUE.as_slice()))
            .collect::<Vec<_>>();
        store.write(&rows, true);
    }
}

fn exercise_aged_readers<S: Store + Sync>() {
    use std::sync::mpsc;
    use std::time::Duration;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let store = S::open(&path);
    seed_history(&store);
    let keys = [
        b"turn:event".as_slice(),
        b"turn:index",
        b"turn:outbox",
        b"turn:projection",
        b"turn:receipt",
    ];
    let write_rows = |revision: u32| {
        keys.iter()
            .map(|&key| (key, revision.to_be_bytes()))
            .collect::<Vec<_>>()
    };
    let assert_revision = |reader: &S::Reader, revision: u32| {
        let actual = reader.range(b"turn:", b"turn;");
        assert_eq!(actual.len(), keys.len());
        for ((key, value), expected_key) in actual.iter().zip(keys) {
            assert_eq!(key, expected_key);
            assert_eq!(value, &revision.to_be_bytes());
        }
    };
    let insert = |revision| {
        let owned = write_rows(revision);
        let rows = owned
            .iter()
            .map(|(key, value)| (*key, value.as_slice()))
            .collect::<Vec<_>>();
        store.write(&rows, true);
    };
    insert(0);
    let original = store.read();
    assert_revision(&original, 0);

    for revision in 1..=16 {
        std::thread::scope(|scope| {
            let (ready, staged) = mpsc::channel();
            let (finish, released) = mpsc::channel();
            let writer = scope.spawn(|| {
                let owned = write_rows(revision);
                let rows = owned
                    .iter()
                    .map(|(key, value)| (*key, value.as_slice()))
                    .collect::<Vec<_>>();
                store.write_before_commit(&rows, true, move || {
                    ready.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(10)).unwrap();
                });
            });
            staged.recv_timeout(Duration::from_secs(10)).unwrap();
            let before = store.read();
            assert_revision(&before, revision - 1);
            let history = before.range(b"history:004096", b"history:004128");
            assert_eq!(history.len(), 32);
            assert_eq!(history[0].0, b"history:004096");
            assert!(history.iter().all(|(_, value)| value == &HISTORY_VALUE));
            finish.send(()).unwrap();
            writer.join().unwrap();
            assert_revision(&before, revision - 1);
            assert_revision(&store.read(), revision);
        });
    }
    assert_revision(&original, 0);
    drop(original);
    drop(store);
    assert_revision(&S::open(&path).read(), 16);
}

#[test]
fn aged_stores_keep_foreground_reads_coherent_across_staged_writes() {
    exercise_aged_readers::<local::Database>();
    exercise_aged_readers::<direct::Database>();
    exercise_aged_readers::<Fjall>();
    exercise_aged_readers::<Sqlite>();
}

fn history_rows(start: u32, end: u32) -> Rows {
    (start..end)
        .map(|id| {
            (
                format!("history:{id:06}").into_bytes(),
                HISTORY_VALUE.to_vec(),
            )
        })
        .collect()
}

fn exercise_retention_snapshots<S: Store + Sync>() {
    use std::sync::mpsc;
    use std::time::Duration;

    const ROUNDS: u32 = 8;
    const PAGE: u32 = 32;
    const HISTORY: u32 = 8_192;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let store = S::open(&path);
    seed_history(&store);
    let revision_rows = |revision: u32| -> Rows {
        ["event", "index", "outbox", "projection", "receipt"]
            .map(|name| {
                (
                    format!("turn:{name}").into_bytes(),
                    revision.to_be_bytes().to_vec(),
                )
            })
            .into()
    };
    let initial = revision_rows(0);
    let rows = initial
        .iter()
        .map(|(key, value)| (key.as_slice(), value.as_slice()))
        .collect::<Vec<_>>();
    store.write(&rows, true);

    let assert_snapshot = |reader: &S::Reader, revision: u32| {
        assert_eq!(reader.range(b"turn:", b"turn;"), revision_rows(revision));
        assert_eq!(
            reader.range(b"history:000000", b"history:000256"),
            history_rows(revision * PAGE, ROUNDS * PAGE),
        );
        assert_eq!(
            reader.range(b"history:008192", b"history:008448"),
            history_rows(HISTORY, HISTORY + revision * PAGE),
        );
    };
    let original = store.read();
    assert_snapshot(&original, 0);
    for revision in 1..=ROUNDS {
        let mut next = revision_rows(revision);
        next.extend(history_rows(
            HISTORY + (revision - 1) * PAGE,
            HISTORY + revision * PAGE,
        ));
        let rows = next
            .iter()
            .map(|(key, value)| (key.as_slice(), value.as_slice()))
            .collect::<Vec<_>>();
        let removed = history_rows((revision - 1) * PAGE, revision * PAGE);
        let removed = removed
            .iter()
            .map(|(key, _)| key.as_slice())
            .collect::<Vec<_>>();

        // Aborting a retention commit must preserve its old page and all root records.
        store.mutate_before_commit(&rows, &removed, false, || {});
        assert_snapshot(&store.read(), revision - 1);

        std::thread::scope(|scope| {
            let (ready, staged) = mpsc::channel();
            let (finish, released) = mpsc::channel();
            let writer = scope.spawn(|| {
                store.mutate_before_commit(&rows, &removed, true, move || {
                    ready.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(10)).unwrap();
                });
            });
            staged.recv_timeout(Duration::from_secs(10)).unwrap();
            let before = store.read();
            assert_snapshot(&before, revision - 1);
            finish.send(()).unwrap();
            writer.join().unwrap();
            assert_snapshot(&before, revision - 1);
            assert_snapshot(&store.read(), revision);
            assert_snapshot(&original, 0);
        });
    }
    drop(original);
    drop(store);

    let reopened = S::open(&path);
    let reader = reopened.read();
    assert_snapshot(&reader, ROUNDS);
    assert_eq!(
        reader.range(b"history:", b"history;"),
        history_rows(ROUNDS * PAGE, HISTORY + ROUNDS * PAGE),
    );
}

#[test]
fn aged_snapshots_keep_pruned_pages_while_new_readers_advance() {
    exercise_retention_snapshots::<local::Database>();
    exercise_retention_snapshots::<direct::Database>();
    exercise_retention_snapshots::<Fjall>();
    exercise_retention_snapshots::<Sqlite>();
}

fn sample_ordered_commits<S: Store>(round: usize, engine: &str) {
    const COMMITS: u32 = 128;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let store = S::open(&path);
    seed_history(&store);
    let pinned = store.read();
    let window = |reader: &S::Reader| reader.range(b"history:004096", b"history:004128");
    let expected_window = (4_096..4_128)
        .map(|index| {
            (
                format!("history:{index:06}").into_bytes(),
                HISTORY_VALUE.to_vec(),
            )
        })
        .collect::<Rows>();
    assert_eq!(window(&pinned), expected_window);

    let mut samples = Vec::new();
    let mut expected = Vec::new();
    for sequence in 1..=COMMITS {
        let key = format!("turn:event:{sequence:06}");
        let index = format!("turn:index:{sequence:06}");
        let receipt = format!("turn:receipt:{sequence:06}");
        let effect = format!("turn:outbox:{sequence:06}");
        let revision = sequence.to_be_bytes();
        let mut payload = HISTORY_VALUE;
        payload[..revision.len()].copy_from_slice(&revision);
        let rows: &[Row<'_>] = &[
            (key.as_bytes(), &payload),
            (index.as_bytes(), &revision),
            (receipt.as_bytes(), b"accepted"),
            (effect.as_bytes(), b"pending"),
            (b"turn:projection", &revision),
        ];
        let commit_started = Instant::now();
        store.write(rows, true);
        let commit_ns = commit_started.elapsed().as_nanos();
        let read_started = Instant::now();
        let visible = window(&pinned);
        let window_ns = read_started.elapsed().as_nanos();
        assert_eq!(visible, expected_window);
        expected.extend(
            rows[..4]
                .iter()
                .map(|&(key, value)| (key.to_vec(), value.to_vec())),
        );
        samples.push((sequence, commit_ns, window_ns));
    }
    assert!(pinned.range(b"turn:", b"turn;").is_empty());
    drop(pinned);
    drop(store);
    let reopened = S::open(&path);
    let snapshot = reopened.read();
    expected.push((b"turn:projection".to_vec(), COMMITS.to_be_bytes().to_vec()));
    expected.sort();
    assert_eq!(snapshot.range(b"turn:", b"turn;"), expected);
    for (sequence, commit_ns, window_ns) in samples {
        println!("ordered-sample\t{round}\t{engine}\t{sequence}\t{commit_ns}\t{window_ns}");
    }
}

#[test]
#[ignore = "Run the release measurement on a dedicated runner"]
fn sample_strict_durable_ordered_commits() {
    for round in 0..8 {
        for position in 0..4 {
            match (round + position) % 4 {
                0 => sample_ordered_commits::<local::Database>(round, "omdb-immediate"),
                1 => sample_ordered_commits::<direct::Database>(round, "redb-immediate"),
                2 => sample_ordered_commits::<Fjall>(round, "fjall-sync-all"),
                _ => sample_ordered_commits::<Sqlite>(round, "sqlite-wal-full"),
            }
        }
    }
}

#[cfg(unix)]
mod process_recovery {
    use super::{
        Fjall, Path, Row, Rows, Snapshot, Sqlite, Store, direct, history_rows, local, seed_history,
    };
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Duration;

    const ACKNOWLEDGED: &[Row<'_>] = &[
        (b"1:event", b"acknowledged"),
        (b"2:projection", b"visible"),
        (b"3:index", b"visible"),
        (b"4:receipt", b"accepted"),
        (b"5:outbox", b"pending"),
    ];
    const INTERRUPTED: &[Row<'_>] = &[
        (b"1:event", b"interrupted"),
        (b"2:projection", b"uncommitted"),
        (b"3:index", b"uncommitted"),
        (b"4:receipt", b"uncommitted"),
        (b"5:outbox", b"uncommitted"),
        (b"5:outbox-duplicate", b"uncommitted"),
    ];
    const READY: &str = "strict-store-write-staged";
    const ENGINE: &str = "T3_ORDERED_RECOVERY_FIXTURE_ENGINE";
    const PATH: &str = "T3_ORDERED_RECOVERY_FIXTURE_PATH";
    const RETENTION: &str = "T3_ORDERED_RECOVERY_FIXTURE_RETENTION";

    fn stage_and_block<S: Store>(path: &Path) {
        let store = S::open(path);
        store.write(ACKNOWLEDGED, true);
        store.write_before_commit(INTERRUPTED, false, signal_and_wait);
    }

    fn signal_and_wait() {
        writeln!(io::stdout().lock(), "{READY}").unwrap();
        io::stdout().flush().unwrap();
        // The parent owns the pipe and kills this writer with the transaction open.
        let _ = io::stdin().lock().read_exact(&mut [0]);
        panic!("fixture writer released without a process kill");
    }

    fn retention_write<S: Store>(
        store: &S,
        root: &[Row<'_>],
        revision: u32,
        commit: bool,
        staged: impl FnOnce(),
    ) {
        let appended = history_rows(8_192 + (revision - 1) * 32, 8_192 + revision * 32);
        let mut rows = root.to_vec();
        rows.extend(
            appended
                .iter()
                .map(|(key, value)| (key.as_slice(), value.as_slice())),
        );
        let removed = history_rows((revision - 1) * 32, revision * 32);
        let removed = removed
            .iter()
            .map(|(key, _)| key.as_slice())
            .collect::<Vec<_>>();
        store.mutate_before_commit(&rows, &removed, commit, staged);
    }

    fn stage_retention_and_block<S: Store>(path: &Path) {
        let store = S::open(path);
        seed_history(&store);
        retention_write(&store, ACKNOWLEDGED, 1, true, || {});
        retention_write(&store, INTERRUPTED, 2, false, signal_and_wait);
    }

    fn stage_fixture<S: Store>(path: &Path) {
        match std::env::var(RETENTION).as_deref() {
            Ok("0") => stage_and_block::<S>(path),
            Ok("1") => stage_retention_and_block::<S>(path),
            _ => panic!("unknown ordered transaction fixture workload"),
        }
    }

    #[test]
    fn writer_fixture() {
        let Some(engine) = std::env::var_os(ENGINE) else {
            return;
        };
        let path = std::env::var_os(PATH).unwrap();
        let path = Path::new(&path);
        match engine.to_str().unwrap() {
            "omdb" => stage_fixture::<local::Database>(path),
            "redb" => stage_fixture::<direct::Database>(path),
            "fjall" => stage_fixture::<Fjall>(path),
            "sqlite" => stage_fixture::<Sqlite>(path),
            _ => panic!("unknown ordered transaction fixture engine"),
        }
    }

    fn interrupt_writer<S: Store>(engine: &str, retention: bool) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_recovery::writer_fixture", "--nocapture"])
            .env(ENGINE, engine)
            .env(PATH, &path)
            .env(RETENTION, if retention { "1" } else { "0" })
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (observed, barrier) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                if line.unwrap().trim() == READY {
                    observed.send(()).ok();
                    break;
                }
            }
        });
        let staged = barrier.recv_timeout(Duration::from_secs(20));
        let killed = child.kill();
        let status = child.wait().unwrap();
        reader.join().unwrap();
        assert!(
            staged.is_ok(),
            "{engine} did not acknowledge the staged fixture"
        );
        killed.unwrap();
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(status.signal(), Some(9));

        let reopened = S::open(&path);
        let actual = reopened.read().range(b"1:", b"6:");
        let expected: Rows = ACKNOWLEDGED
            .iter()
            .map(|&(key, value)| (key.to_vec(), value.to_vec()))
            .collect();
        assert_eq!(
            actual, expected,
            "{engine} did not preserve the root commit"
        );
        if retention {
            assert_eq!(
                reopened.read().range(b"history:", b"history;"),
                history_rows(32, 8_224),
                "{engine} did not preserve exactly the acknowledged retention root",
            );
        }
    }

    #[test]
    fn strict_stores_recover_acknowledged_commit_after_process_death() {
        interrupt_writer::<local::Database>("omdb", false);
        interrupt_writer::<direct::Database>("redb", false);
        interrupt_writer::<Fjall>("fjall", false);
        interrupt_writer::<Sqlite>("sqlite", false);
    }

    #[test]
    fn strict_stores_recover_acknowledged_retention_after_process_death() {
        interrupt_writer::<local::Database>("omdb", true);
        interrupt_writer::<direct::Database>("redb", true);
        interrupt_writer::<Fjall>("fjall", true);
        interrupt_writer::<Sqlite>("sqlite", true);
    }
}
