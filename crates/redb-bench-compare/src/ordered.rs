//! Prospective strict-durability adapters for the ordered comparison store.
//! Select an engine before opening its dedicated comparison path. The caller
//! owns the path, record encoding, and any application-level admission checks.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::path::Path;
use std::sync::Arc;

use fjall_contract::{PersistMode, Readable as _};
use redb_direct::{ReadableDatabase as _, ReadableTable as _};

const NAME: &str = "ordered";
const TABLE: redb_direct::TableDefinition<&[u8], &[u8]> = redb_direct::TableDefinition::new(NAME);
const MAX_KEY_BYTES: usize = u16::MAX as usize;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_RECORDS: usize = 65_536;
const MAX_SAVEPOINT_DEPTH: usize = 4;

type Writes = BTreeMap<Arc<[u8]>, Option<Arc<[u8]>>>;
pub type Rows = Vec<(Vec<u8>, Vec<u8>)>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Engine {
    Fjall,
    DirectRedb,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    InvalidKey,
    InvalidPage,
    BudgetExceeded,
    /// The operation failed before the batch could be committed.
    Precommit(Box<dyn StdError + Send + Sync>),
    /// Reconcile against recovered state before retrying or publishing effects.
    UnknownCommit(Box<dyn StdError + Send + Sync>),
}

impl Error {
    fn precommit(error: impl StdError + Send + Sync + 'static) -> Self {
        Self::Precommit(Box::new(error))
    }

    fn unknown_commit(error: impl StdError + Send + Sync + 'static) -> Self {
        Self::UnknownCommit(Box::new(error))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKey => out.write_str("invalid ordered store key length"),
            Self::InvalidPage => out.write_str("invalid ordered store page"),
            Self::BudgetExceeded => out.write_str("ordered transaction budget exceeded"),
            Self::Precommit(cause) => write!(out, "ordered store operation failed: {cause}"),
            Self::UnknownCommit(cause) => write!(out, "ordered commit outcome is unknown: {cause}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Precommit(cause) | Self::UnknownCommit(cause) => Some(cause.as_ref()),
            _ => None,
        }
    }
}

pub struct Database {
    inner: OpenDatabase,
}

enum OpenDatabase {
    Fjall(
        fjall_contract::SingleWriterTxDatabase,
        fjall_contract::SingleWriterTxKeyspace,
    ),
    DirectRedb(redb_direct::Database),
}

impl Database {
    /// Open or create a comparison database with the selected engine.
    pub fn open(engine: Engine, path: &Path) -> Result<Self, Error> {
        let inner = match engine {
            Engine::Fjall => {
                let db = fjall_contract::SingleWriterTxDatabase::builder(path)
                    .open()
                    .map_err(Error::precommit)?;
                let keys = db
                    .keyspace(NAME, Default::default)
                    .map_err(Error::precommit)?;
                OpenDatabase::Fjall(db, keys)
            }
            Engine::DirectRedb => {
                let db = redb_direct::Database::create(path).map_err(Error::precommit)?;
                let txn = begin_redb_write(&db)?;
                txn.open_table(TABLE).map_err(Error::precommit)?;
                txn.commit().map_err(redb_commit_error)?;
                OpenDatabase::DirectRedb(db)
            }
        };
        Ok(Self { inner })
    }

    pub fn snapshot(&self) -> Result<Snapshot, Error> {
        let inner = match &self.inner {
            OpenDatabase::Fjall(db, keys) => ReadSnapshot::Fjall(db.read_tx(), keys.clone()),
            OpenDatabase::DirectRedb(db) => {
                ReadSnapshot::DirectRedb(db.begin_read().map_err(Error::precommit)?)
            }
        };
        Ok(Snapshot { inner })
    }

    /// Acquire the engine's writer before taking the read snapshot. The callback
    /// runs synchronously on the writer's thread and must not acquire that writer again.
    /// An error discards the batch. Publish external effects only after this
    /// method returns the callback result following strict commit.
    pub fn write<T, E: From<Error>>(
        &self,
        operation: impl FnOnce(&mut WriteTransaction) -> Result<T, E>,
    ) -> Result<T, E> {
        match &self.inner {
            OpenDatabase::Fjall(db, keys) => {
                let mut txn = db.write_tx().durability(Some(PersistMode::SyncAll));
                self.write_staged(operation, move |writes| {
                    for (key, value) in writes {
                        match value {
                            Some(value) => txn.insert(keys, key.as_ref(), value.as_ref()),
                            None => txn.remove(keys, key.as_ref()),
                        }
                    }
                    txn.commit().map_err(|error| match error {
                        fjall_contract::Error::Poisoned => Error::precommit(error),
                        _ => Error::unknown_commit(error),
                    })
                })
            }
            OpenDatabase::DirectRedb(db) => {
                let txn = begin_redb_write(db)?;
                self.write_staged(operation, move |writes| {
                    {
                        let mut table = txn.open_table(TABLE).map_err(Error::precommit)?;
                        for (key, value) in writes {
                            match value {
                                Some(value) => table
                                    .insert(key.as_ref(), value.as_ref())
                                    .map_err(Error::precommit)?,
                                None => table.remove(key.as_ref()).map_err(Error::precommit)?,
                            };
                        }
                    }
                    txn.commit().map_err(redb_commit_error)
                })
            }
        }
    }

    fn write_staged<T, E: From<Error>>(
        &self,
        operation: impl FnOnce(&mut WriteTransaction) -> Result<T, E>,
        commit: impl FnOnce(Writes) -> Result<(), Error>,
    ) -> Result<T, E> {
        let mut staged = WriteTransaction {
            snapshot: self.snapshot()?,
            writes: BTreeMap::new(),
            bytes: 0,
            savepoint_depth: 0,
        };
        // Release Fjall's native mutex outside callback unwind to keep the writer usable.
        let result = match catch_unwind(AssertUnwindSafe(|| operation(&mut staged))) {
            Ok(result) => result?,
            Err(panic) => {
                drop(commit);
                resume_unwind(panic);
            }
        };
        let WriteTransaction {
            snapshot, writes, ..
        } = staged;
        drop(snapshot);
        if !writes.is_empty() {
            commit(writes)?;
        }
        Ok(result)
    }
}

fn begin_redb_write(db: &redb_direct::Database) -> Result<redb_direct::WriteTransaction, Error> {
    let mut txn = db.begin_write().map_err(Error::precommit)?;
    txn.set_durability(redb_direct::Durability::Immediate)
        .map_err(Error::precommit)?;
    Ok(txn)
}

fn redb_commit_error(error: redb_direct::CommitError) -> Error {
    match error {
        redb_direct::CommitError::TransactionPoisoned => Error::precommit(error),
        _ => Error::unknown_commit(error),
    }
}

pub struct Snapshot {
    inner: ReadSnapshot,
}

enum ReadSnapshot {
    Fjall(
        fjall_contract::Snapshot,
        fjall_contract::SingleWriterTxKeyspace,
    ),
    DirectRedb(redb_direct::ReadTransaction),
}

impl Snapshot {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        check_key(key)?;
        let copy = |value: &[u8]| {
            if key.len().saturating_add(value.len()) > MAX_BYTES {
                return Err(Error::BudgetExceeded);
            }
            Ok(value.to_vec())
        };
        match &self.inner {
            ReadSnapshot::Fjall(txn, keys) => {
                let value = txn.get(keys, key).map_err(Error::precommit)?;
                value.as_deref().map(copy).transpose()
            }
            ReadSnapshot::DirectRedb(txn) => {
                let table = txn.open_table(TABLE).map_err(Error::precommit)?;
                let value = table.get(key).map_err(Error::precommit)?;
                value.map(|value| copy(value.value())).transpose()
            }
        }
    }

    /// Return at most `limit` records in byte order in `[start, end)`.
    /// The page rejects more than 16 MiB of key and value bytes.
    pub fn scan(&self, start: &[u8], end: &[u8], limit: usize) -> Result<Rows, Error> {
        check_page(start, end, limit)?;
        self.scan_with(start, end, limit, std::iter::empty())
    }

    fn scan_with<'a>(
        &self,
        start: &[u8],
        end: &[u8],
        limit: usize,
        pending: impl Iterator<Item = (&'a Arc<[u8]>, &'a Option<Arc<[u8]>>)>,
    ) -> Result<Rows, Error> {
        let mut pending = pending.peekable();
        let mut page = Page {
            rows: Vec::new(),
            bytes: 0,
        };
        self.visit(start, end, |key, value| {
            while let Some((staged_key, staged_value)) =
                pending.next_if(|(staged_key, _)| staged_key.as_ref() <= key)
            {
                if let Some(staged_value) = staged_value {
                    page.push(staged_key, staged_value)?;
                }
                if staged_key.as_ref() == key || page.rows.len() == limit {
                    return Ok(page.rows.len() < limit);
                }
            }
            page.push(key, value)?;
            Ok(page.rows.len() < limit)
        })?;
        for (key, value) in pending {
            if page.rows.len() == limit {
                break;
            }
            if let Some(value) = value {
                page.push(key, value)?;
            }
        }
        Ok(page.rows)
    }

    fn visit(
        &self,
        start: &[u8],
        end: &[u8],
        mut visit: impl FnMut(&[u8], &[u8]) -> Result<bool, Error>,
    ) -> Result<(), Error> {
        match &self.inner {
            ReadSnapshot::Fjall(txn, keys) => {
                for row in txn.range(keys, start..end) {
                    let (key, value) = row.into_inner().map_err(Error::precommit)?;
                    if !visit(&key, &value)? {
                        break;
                    }
                }
            }
            ReadSnapshot::DirectRedb(txn) => {
                let table = txn.open_table(TABLE).map_err(Error::precommit)?;
                for row in table.range(start..end).map_err(Error::precommit)? {
                    let (key, value) = row.map_err(Error::precommit)?;
                    if !visit(key.value(), value.value())? {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

struct Page {
    rows: Rows,
    bytes: usize,
}

impl Page {
    fn push(&mut self, key: &[u8], value: &[u8]) -> Result<(), Error> {
        self.bytes = self
            .bytes
            .saturating_add(key.len())
            .saturating_add(value.len());
        if self.bytes > MAX_BYTES {
            return Err(Error::BudgetExceeded);
        }
        self.rows.push((key.to_vec(), value.to_vec()));
        Ok(())
    }
}

/// One staged root batch, limited to 65,536 distinct keys and 16 MiB of key and value bytes.
pub struct WriteTransaction {
    snapshot: Snapshot,
    writes: Writes,
    bytes: usize,
    savepoint_depth: usize,
}

impl WriteTransaction {
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Error> {
        match self.writes.get(key) {
            Some(value) => Ok(value.as_deref().map(<[u8]>::to_vec)),
            None => self.snapshot.get(key),
        }
    }

    pub fn scan(&self, start: &[u8], end: &[u8], limit: usize) -> Result<Rows, Error> {
        check_page(start, end, limit)?;
        self.snapshot.scan_with(
            start,
            end,
            limit,
            self.writes.range::<[u8], _>((
                std::ops::Bound::Included(start),
                std::ops::Bound::Excluded(end),
            )),
        )
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> Result<(), Error> {
        self.stage(key, Some(value))
    }

    pub fn delete(&mut self, key: &[u8]) -> Result<(), Error> {
        self.stage(key, None)
    }

    fn stage(&mut self, key: &[u8], value: Option<&[u8]>) -> Result<(), Error> {
        check_key(key)?;
        let previous = self.writes.get(key);
        let old_bytes = previous.map_or(0, |value| {
            key.len() + value.as_deref().map_or(0, <[u8]>::len)
        });
        let bytes = (self.bytes - old_bytes)
            .saturating_add(key.len())
            .saturating_add(value.map_or(0, <[u8]>::len));
        if bytes > MAX_BYTES || (previous.is_none() && self.writes.len() == MAX_RECORDS) {
            return Err(Error::BudgetExceeded);
        }
        self.writes.insert(Arc::from(key), value.map(Arc::from));
        self.bytes = bytes;
        Ok(())
    }

    /// Roll back only this scope on error. Successful scopes remain staged until
    /// the root commits. Up to four active scopes share their unchanged record bytes.
    pub fn savepoint<T, E: From<Error>>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, E>,
    ) -> Result<T, E> {
        if self.savepoint_depth == MAX_SAVEPOINT_DEPTH {
            return Err(Error::BudgetExceeded.into());
        }
        let checkpoint = (self.writes.clone(), self.bytes);
        self.savepoint_depth += 1;
        let result = catch_unwind(AssertUnwindSafe(|| operation(self)));
        self.savepoint_depth -= 1;
        if !matches!(result, Ok(Ok(_))) {
            (self.writes, self.bytes) = checkpoint;
        }
        result.unwrap_or_else(|panic| resume_unwind(panic))
    }
}

fn check_key(key: &[u8]) -> Result<(), Error> {
    if key.is_empty() || key.len() > MAX_KEY_BYTES {
        return Err(Error::InvalidKey);
    }
    Ok(())
}

fn check_page(start: &[u8], end: &[u8], limit: usize) -> Result<(), Error> {
    if start >= end
        || end.len() > MAX_KEY_BYTES
        || start.len() > MAX_KEY_BYTES
        || !(1..=MAX_RECORDS).contains(&limit)
    {
        return Err(Error::InvalidPage);
    }
    Ok(())
}
