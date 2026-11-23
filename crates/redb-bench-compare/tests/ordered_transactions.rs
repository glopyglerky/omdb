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
    fn write(&self, rows: &[Row<'_>], commit: bool);
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

                fn write(&self, rows: &[Row<'_>], commit: bool) {
                    let mut txn = self.0.begin_write().unwrap();
                    txn.set_durability($engine::Durability::Immediate).unwrap();
                    {
                        let mut table = txn.open_table(TABLE).unwrap();
                        for &(key, value) in rows {
                            table.insert(key, value).unwrap();
                        }
                    }
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

    fn write(&self, rows: &[Row<'_>], commit: bool) {
        let mut txn = self
            .db
            .write_tx()
            .durability(Some(fjall_contract::PersistMode::SyncAll));
        for &(key, value) in rows {
            txn.insert(&self.keys, key, value);
        }
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

    fn write(&self, rows: &[Row<'_>], commit: bool) {
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
