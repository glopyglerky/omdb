use std::path::{Path, PathBuf};

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
    fn write_before_commit(&self, rows: &[Row<'_>], commit: bool, staged: impl FnOnce());

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

                fn write_before_commit(
                    &self,
                    rows: &[Row<'_>],
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

    fn write_before_commit(&self, rows: &[Row<'_>], commit: bool, staged: impl FnOnce()) {
        let mut txn = self
            .db
            .write_tx()
            .durability(Some(fjall_contract::PersistMode::SyncAll));
        for &(key, value) in rows {
            txn.insert(&self.keys, key, value);
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

struct Sqlite(PathBuf);

impl Sqlite {
    fn connect(&self) -> Connection {
        let conn = Connection::open(&self.0).unwrap();
        conn.execute_batch("PRAGMA synchronous=FULL;").unwrap();
        conn
    }
}

impl Store for Sqlite {
    type Reader = Connection;

    fn open(path: &Path) -> Self {
        let store = Self(path.to_owned());
        store
            .connect()
            .execute_batch(
                "PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS ordered
             (key BLOB PRIMARY KEY, value BLOB NOT NULL) WITHOUT ROWID;",
            )
            .unwrap();
        store
    }

    fn read(&self) -> Self::Reader {
        let conn = self.connect();
        conn.execute_batch("BEGIN DEFERRED;").unwrap();
        conn
    }

    fn write_before_commit(&self, rows: &[Row<'_>], commit: bool, staged: impl FnOnce()) {
        let mut conn = self.connect();
        let txn = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .unwrap();
        for &(key, value) in rows {
            txn.execute(
                "INSERT OR REPLACE INTO ordered VALUES (?, ?)",
                params![key, value],
            )
            .unwrap();
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
            .prepare("SELECT key, value FROM ordered WHERE key >= ? AND key < ? ORDER BY key")
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

fn exercise_aged_readers<S: Store + Sync>() {
    use std::sync::mpsc;
    use std::time::Duration;

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let store = S::open(&path);
    let cold_value = [0x35; 1_024];
    for start in (0..8_192u32).step_by(256) {
        let keys = (start..start + 256)
            .map(|id| format!("history:{id:06}"))
            .collect::<Vec<_>>();
        let rows = keys
            .iter()
            .map(|key| (key.as_bytes(), cold_value.as_slice()))
            .collect::<Vec<_>>();
        store.write(&rows, true);
    }
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
            assert!(history.iter().all(|(_, value)| value == &cold_value));
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

#[cfg(unix)]
mod process_recovery {
    use super::{Fjall, Path, Row, Rows, Snapshot, Sqlite, Store, direct, local};
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

    fn stage_and_block<S: Store>(path: &Path) {
        let store = S::open(path);
        store.write(ACKNOWLEDGED, true);
        store.write_before_commit(INTERRUPTED, false, || {
            writeln!(io::stdout().lock(), "{READY}").unwrap();
            io::stdout().flush().unwrap();
            // The parent owns the pipe and kills this writer with the transaction open.
            let _ = io::stdin().lock().read_exact(&mut [0]);
            panic!("fixture writer released without a process kill");
        });
    }

    #[test]
    fn writer_fixture() {
        let Some(engine) = std::env::var_os(ENGINE) else {
            return;
        };
        let path = std::env::var_os(PATH).unwrap();
        let path = Path::new(&path);
        match engine.to_str().unwrap() {
            "omdb" => stage_and_block::<local::Database>(path),
            "redb" => stage_and_block::<direct::Database>(path),
            "fjall" => stage_and_block::<Fjall>(path),
            "sqlite" => stage_and_block::<Sqlite>(path),
            _ => panic!("unknown ordered transaction fixture engine"),
        }
    }

    fn interrupt_writer<S: Store>(engine: &str) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_recovery::writer_fixture", "--nocapture"])
            .env(ENGINE, engine)
            .env(PATH, &path)
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
    }

    #[test]
    fn strict_stores_recover_acknowledged_commit_after_process_death() {
        interrupt_writer::<local::Database>("omdb");
        interrupt_writer::<direct::Database>("redb");
        interrupt_writer::<Fjall>("fjall");
        interrupt_writer::<Sqlite>("sqlite");
    }
}
