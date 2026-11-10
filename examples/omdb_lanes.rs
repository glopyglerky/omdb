//! Experimental hosted lane persistence, using the unchanged redb engine.
//!
//! Each lane owns a different database. Numeric caller IDs stand for claims an
//! embedding application has already authenticated; they do not authenticate anyone.
//! This example has no scheduler, network consensus, hardware driver, or bare-metal
//! boot path. An external adapter must enforce the epoch and monotonic per-resource
//! sequence or FIFO at the point of effect; otherwise an older intent can regress
//! the device. Recording an effect intent does not make physical effects exactly-once.
//! Revoked intents remain recorded and require a future reconciliation policy.

use redb::{
    Database, Durability, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction,
};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::Path;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

// State: lane, owner, epoch, version, value.
const STATE: TableDefinition<u8, [u64; 5]> = TableDefinition::new("omdb_example_state_v1");
// Receipt: original command fields followed by resulting version and value.
const RECEIPTS: TableDefinition<u64, [u64; 7]> = TableDefinition::new("omdb_example_receipts_v1");
// Effect: lane, owner, epoch, resulting version, desired value.
const EFFECTS: TableDefinition<u64, [u64; 5]> = TableDefinition::new("omdb_example_effects_v1");
const MAX_AMOUNT: u64 = 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rejected {
    Busy,
    Unauthorized,
    WrongLane,
    StaleEpoch,
    StaleVersion,
    ReusedId,
    InvalidInput,
    Overflow,
    MissingState,
    InvalidEffect,
}

impl Display for Rejected {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "lane request rejected: {self:?}")
    }
}

impl Error for Rejected {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Command {
    lane: u64,
    owner: u64,
    epoch: u64,
    id: u64,
    expected_version: u64,
    amount: u64,
}

impl Command {
    fn fields(self) -> [u64; 5] {
        [
            self.lane,
            self.owner,
            self.epoch,
            self.expected_version,
            self.amount,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Receipt {
    version: u64,
    value: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Effect {
    id: u64,
    fields: [u64; 5],
}

struct Lane {
    db: Database,
}

fn authorized(state: [u64; 5], caller: u64) -> Result<()> {
    if caller == 0 || state[1] != caller {
        return Err(Rejected::Unauthorized.into());
    }
    Ok(())
}

fn fenced(state: [u64; 5], lane: u64, owner: u64, epoch: u64) -> Result<()> {
    if state[0] != lane {
        return Err(Rejected::WrongLane.into());
    }
    if state[1] != owner {
        return Err(Rejected::Unauthorized.into());
    }
    if state[2] != epoch {
        return Err(Rejected::StaleEpoch.into());
    }
    Ok(())
}

impl Lane {
    fn open(path: &Path, lane: u64, owner: u64) -> Result<Self> {
        if lane == 0 || owner == 0 {
            return Err(Rejected::InvalidInput.into());
        }
        let db = Database::create(path)?;
        let tx = db.begin_write()?;
        {
            let mut state = tx.open_table(STATE)?;
            let existing = state.get(0)?.map(|v| v.value());
            if let Some(existing) = existing {
                authorized(existing, owner)?;
                if existing[0] != lane {
                    return Err(Rejected::WrongLane.into());
                }
            } else {
                state.insert(0, [lane, owner, 1, 0, 0])?;
            }
            tx.open_table(RECEIPTS)?;
            tx.open_table(EFFECTS)?;
        }
        tx.commit()?;
        Ok(Self { db })
    }

    fn begin(&self) -> Result<WriteTransaction> {
        match self.db.try_begin_write() {
            Ok(mut tx) => {
                tx.set_durability(Durability::Immediate)?;
                Ok(tx)
            }
            Err(redb::TransactionError::WouldBlock) => Err(Rejected::Busy.into()),
            Err(error) => Err(error.into()),
        }
    }

    fn snapshot(&self, caller: u64) -> Result<[u64; 5]> {
        let tx = self.db.begin_read()?;
        let table = tx.open_table(STATE)?;
        let state = table.get(0)?.ok_or(Rejected::MissingState)?.value();
        authorized(state, caller)?;
        Ok(state)
    }

    // Separate staging permits testing interruption before the commit. No effect
    // executes here; state, full original request, receipt, and intent commit together.
    fn stage(tx: &WriteTransaction, caller: u64, command: Command) -> Result<Receipt> {
        let mut state_table = tx.open_table(STATE)?;
        let mut state = state_table.get(0)?.ok_or(Rejected::MissingState)?.value();
        authorized(state, caller)?;
        fenced(state, command.lane, command.owner, command.epoch)?;
        if command.id == 0 || command.amount == 0 || command.amount > MAX_AMOUNT {
            return Err(Rejected::InvalidInput.into());
        }

        let mut receipts = tx.open_table(RECEIPTS)?;
        if let Some(receipt) = receipts.get(command.id)? {
            let row = receipt.value();
            if row[..5] != command.fields() {
                return Err(Rejected::ReusedId.into());
            }
            // Authorization and epoch are checked even for a retry. The original
            // expected version is compared to its receipt, not to the newer state.
            return Ok(Receipt {
                version: row[5],
                value: row[6],
            });
        }
        if command.expected_version != state[3] {
            return Err(Rejected::StaleVersion.into());
        }
        let version = state[3].checked_add(1).ok_or(Rejected::Overflow)?;
        let value = state[4]
            .checked_add(command.amount)
            .ok_or(Rejected::Overflow)?;
        let request = command.fields();
        receipts.insert(
            command.id,
            [
                request[0], request[1], request[2], request[3], request[4], version, value,
            ],
        )?;
        tx.open_table(EFFECTS)?.insert(
            command.id,
            [command.lane, command.owner, command.epoch, version, value],
        )?;
        state[3] = version;
        state[4] = value;
        state_table.insert(0, state)?;
        Ok(Receipt { version, value })
    }

    fn apply(&self, caller: u64, command: Command) -> Result<Receipt> {
        let tx = self.begin()?;
        let receipt = Self::stage(&tx, caller, command)?;
        tx.commit()?;
        Ok(receipt)
    }

    fn pending_effect(&self, caller: u64, id: u64) -> Result<Option<Effect>> {
        let tx = self.db.begin_read()?;
        let state = tx
            .open_table(STATE)?
            .get(0)?
            .ok_or(Rejected::MissingState)?
            .value();
        authorized(state, caller)?;
        let effect = tx.open_table(EFFECTS)?.get(id)?.map(|v| Effect {
            id,
            fields: v.value(),
        });
        if let Some(effect) = effect {
            fenced(state, effect.fields[0], effect.fields[1], effect.fields[2])?;
        }
        // The owned result carries no live transaction into the external adapter.
        Ok(effect)
    }

    fn complete_effect(&self, caller: u64, effect: Effect) -> Result<bool> {
        let tx = self.begin()?;
        let changed;
        {
            let state = tx
                .open_table(STATE)?
                .get(0)?
                .ok_or(Rejected::MissingState)?
                .value();
            authorized(state, caller)?;
            fenced(state, effect.fields[0], effect.fields[1], effect.fields[2])?;
            let receipt = tx
                .open_table(RECEIPTS)?
                .get(effect.id)?
                .ok_or(Rejected::InvalidEffect)?
                .value();
            if effect.fields != [receipt[0], receipt[1], receipt[2], receipt[5], receipt[6]] {
                return Err(Rejected::InvalidEffect.into());
            }
            let mut pending = tx.open_table(EFFECTS)?;
            changed = if let Some(previous) = pending.remove(effect.id)? {
                if previous.value() != effect.fields {
                    return Err(Rejected::InvalidEffect.into());
                }
                true
            } else {
                false
            };
        }
        tx.commit()?;
        Ok(changed)
    }

    fn transfer(&self, caller: u64, new_owner: u64) -> Result<u64> {
        if new_owner == 0 {
            return Err(Rejected::InvalidInput.into());
        }
        let tx = self.begin()?;
        let epoch;
        {
            let mut table = tx.open_table(STATE)?;
            let mut state = table.get(0)?.ok_or(Rejected::MissingState)?.value();
            authorized(state, caller)?;
            epoch = state[2].checked_add(1).ok_or(Rejected::Overflow)?;
            state[3] = state[3].checked_add(1).ok_or(Rejected::Overflow)?;
            state[1] = new_owner;
            state[2] = epoch;
            table.insert(0, state)?;
        }
        tx.commit()?;
        // Old intents remain for reconciliation. Production must fence the actual
        // adapter before acknowledging an authority transfer to its new owner.
        Ok(epoch)
    }
}

fn main() -> Result<()> {
    let temporary = tempfile::tempdir()?;
    let lanes = [
        Lane::open(&temporary.path().join("lane-1.redb"), 1, 10)?,
        Lane::open(&temporary.path().join("lane-2.redb"), 2, 10)?,
    ];
    for (index, lane) in lanes.iter().enumerate() {
        let lane_id = u64::try_from(index)?
            .checked_add(1)
            .ok_or(Rejected::Overflow)?;
        let command = Command {
            lane: lane_id,
            owner: 10,
            epoch: 1,
            id: 1,
            expected_version: 0,
            amount: 7,
        };
        let receipt = lane.apply(10, command)?;
        assert_eq!(lane.apply(10, command)?, receipt);
        let effect = lane.pending_effect(10, 1)?.ok_or(Rejected::InvalidEffect)?;
        println!(
            "lane {lane_id}: committed {receipt:?}; simulated target {}",
            effect.fields[4]
        );
        assert!(lane.complete_effect(10, effect)?);
        assert!(!lane.complete_effect(10, effect)?);
        assert_eq!(lane.snapshot(10)?[4], 7);
        assert_eq!(lane.transfer(10, 11)?, 2);
    }
    println!("Hosted persistence demonstration only; no physical device was controlled.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as ProcessCommand;

    fn command(lane: u64) -> Command {
        Command {
            lane,
            owner: 10,
            epoch: 1,
            id: 5,
            expected_version: 0,
            amount: 7,
        }
    }

    fn reject<T: std::fmt::Debug>(result: Result<T>, expected: Rejected) {
        assert_eq!(
            result.unwrap_err().downcast_ref::<Rejected>(),
            Some(&expected)
        );
    }

    #[test]
    fn a_busy_lane_does_not_block_another_database() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let a = Lane::open(&directory.path().join("a"), 1, 10)?;
        let b = Lane::open(&directory.path().join("b"), 2, 10)?;
        let occupied = a.begin()?;
        reject(a.apply(10, command(1)), Rejected::Busy);
        assert_eq!(b.apply(10, command(2))?.value, 7);
        occupied.abort()?;
        assert_eq!(a.apply(10, command(1))?.value, 7);
        Ok(())
    }

    #[test]
    fn committed_retry_survives_reopen_without_repeating_state_or_intent() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("lane");
        let lane = Lane::open(&path, 1, 10)?;
        let original = lane.apply(10, command(1))?;
        let effect = lane.pending_effect(10, 5)?.unwrap();
        drop(lane);
        let lane = Lane::open(&path, 1, 10)?;
        assert_eq!(lane.apply(10, command(1))?, original);
        assert_eq!(lane.snapshot(10)?, [1, 10, 1, 1, 7]);
        assert_eq!(lane.pending_effect(10, 5)?, Some(effect));
        assert!(lane.complete_effect(10, effect)?);
        drop(lane);
        let lane = Lane::open(&path, 1, 10)?;
        assert_eq!(lane.apply(10, command(1))?, original);
        assert_eq!(lane.pending_effect(10, 5)?, None);
        assert!(!lane.complete_effect(10, effect)?);
        Ok(())
    }

    #[test]
    fn changed_input_cannot_reuse_a_committed_id() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let lane = Lane::open(&directory.path().join("lane"), 1, 10)?;
        lane.apply(10, command(1))?;
        reject(
            lane.apply(
                10,
                Command {
                    amount: 8,
                    ..command(1)
                },
            ),
            Rejected::ReusedId,
        );
        reject(
            lane.apply(
                10,
                Command {
                    expected_version: 1,
                    ..command(1)
                },
            ),
            Rejected::ReusedId,
        );
        assert_eq!(lane.snapshot(10)?[4], 7);
        Ok(())
    }

    #[test]
    fn retry_and_completion_recheck_current_authority() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let lane = Lane::open(&directory.path().join("lane"), 1, 10)?;
        lane.apply(10, command(1))?;
        let effect = lane.pending_effect(10, 5)?.unwrap();
        assert_eq!(lane.transfer(10, 11)?, 2);
        reject(lane.apply(10, command(1)), Rejected::Unauthorized);
        reject(lane.apply(11, command(1)), Rejected::Unauthorized);
        reject(lane.pending_effect(10, 5), Rejected::Unauthorized);
        reject(lane.complete_effect(10, effect), Rejected::Unauthorized);
        reject(lane.complete_effect(11, effect), Rejected::Unauthorized);
        Ok(())
    }

    #[test]
    fn epoch_revocation_rejects_completion_of_a_previously_retrieved_intent() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let lane = Lane::open(&directory.path().join("lane"), 1, 10)?;
        lane.apply(10, command(1))?;
        let effect = lane.pending_effect(10, 5)?.unwrap();
        lane.transfer(10, 10)?;
        reject(lane.apply(10, command(1)), Rejected::StaleEpoch);
        reject(lane.pending_effect(10, 5), Rejected::StaleEpoch);
        reject(lane.complete_effect(10, effect), Rejected::StaleEpoch);
        Ok(())
    }

    #[test]
    fn abort_rolls_back_state_receipt_and_intent_together() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("lane");
        let lane = Lane::open(&path, 1, 10)?;
        let tx = lane.begin()?;
        Lane::stage(&tx, 10, command(1))?;
        tx.abort()?;
        drop(lane);
        let lane = Lane::open(&path, 1, 10)?;
        assert_eq!(lane.snapshot(10)?, [1, 10, 1, 0, 0]);
        assert_eq!(lane.pending_effect(10, 5)?, None);
        assert_eq!(
            lane.apply(10, command(1))?,
            Receipt {
                version: 1,
                value: 7
            }
        );
        Ok(())
    }

    #[test]
    fn abrupt_process_exit_before_and_after_commit_recovers() -> Result<()> {
        const PATH_ENV: &str = "OMDB_EXAMPLE_CRASH_DATABASE";
        const MODE_ENV: &str = "OMDB_EXAMPLE_CRASH_PHASE";
        if let Some(path) = std::env::var_os(PATH_ENV) {
            let lane = Lane::open(Path::new(&path), 1, 10)?;
            match std::env::var(MODE_ENV)?.as_str() {
                "before" => {
                    let tx = lane.begin()?;
                    Lane::stage(&tx, 10, command(1))?;
                    std::process::exit(73);
                }
                "after" => {
                    lane.apply(10, command(1))?;
                    std::process::exit(74);
                }
                _ => panic!("unknown crash phase"),
            }
        }

        for (phase, exit_code, expected_version) in [("before", 73, 0), ("after", 74, 1)] {
            let directory = tempfile::tempdir()?;
            let path = directory.path().join("lane");
            let output = ProcessCommand::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "tests::abrupt_process_exit_before_and_after_commit_recovers",
                    "--nocapture",
                ])
                .env(PATH_ENV, &path)
                .env(MODE_ENV, phase)
                .output()?;
            assert_eq!(output.status.code(), Some(exit_code), "{output:?}");
            let lane = Lane::open(&path, 1, 10)?;
            assert_eq!(lane.snapshot(10)?[3], expected_version);
            assert_eq!(lane.pending_effect(10, 5)?.is_some(), expected_version == 1);
            assert_eq!(
                lane.apply(10, command(1))?,
                Receipt {
                    version: 1,
                    value: 7
                }
            );
            assert_eq!(lane.snapshot(10)?[4], 7);
        }
        Ok(())
    }

    #[test]
    fn invalid_versions_inputs_and_overflow_leave_no_intent() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let lane = Lane::open(&directory.path().join("lane"), 1, 10)?;
        reject(lane.apply(9, command(1)), Rejected::Unauthorized);
        reject(lane.apply(10, command(2)), Rejected::WrongLane);
        reject(
            lane.apply(
                10,
                Command {
                    id: 0,
                    ..command(1)
                },
            ),
            Rejected::InvalidInput,
        );
        reject(
            lane.apply(
                10,
                Command {
                    amount: MAX_AMOUNT + 1,
                    ..command(1)
                },
            ),
            Rejected::InvalidInput,
        );
        reject(
            lane.apply(
                10,
                Command {
                    expected_version: 1,
                    ..command(1)
                },
            ),
            Rejected::StaleVersion,
        );
        for state in [[1, 10, 1, 0, u64::MAX], [1, 10, 1, u64::MAX, 0]] {
            let tx = lane.begin()?;
            tx.open_table(STATE)?.insert(0, state)?;
            tx.commit()?;
            reject(
                lane.apply(
                    10,
                    Command {
                        expected_version: state[3],
                        ..command(1)
                    },
                ),
                Rejected::Overflow,
            );
            assert_eq!(lane.snapshot(10)?, state);
            assert_eq!(lane.pending_effect(10, 5)?, None);
        }
        Ok(())
    }

    #[test]
    fn effect_runs_after_transaction_and_forged_completion_is_rejected() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let lane = Lane::open(&directory.path().join("lane"), 1, 10)?;
        lane.apply(10, command(1))?;
        let effect = lane.pending_effect(10, 5)?.unwrap();
        // A simulated device can make progress without inheriting the writer.
        lane.begin()?.abort()?;
        let simulated_device_value = effect.fields[4];
        assert_eq!(simulated_device_value, 7);
        let mut forged = effect;
        forged.fields[4] = 8;
        reject(lane.complete_effect(10, forged), Rejected::InvalidEffect);
        assert_eq!(lane.pending_effect(10, 5)?, Some(effect));
        assert!(lane.complete_effect(10, effect)?);
        Ok(())
    }

    #[test]
    fn reopening_cannot_reset_lane_identity_or_ownership() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("lane");
        let lane = Lane::open(&path, 1, 10)?;
        lane.apply(10, command(1))?;
        drop(lane);
        assert_eq!(
            Lane::open(&path, 2, 10)
                .err()
                .unwrap()
                .downcast_ref::<Rejected>(),
            Some(&Rejected::WrongLane)
        );
        assert_eq!(
            Lane::open(&path, 1, 11)
                .err()
                .unwrap()
                .downcast_ref::<Rejected>(),
            Some(&Rejected::Unauthorized)
        );
        assert_eq!(Lane::open(&path, 1, 10)?.snapshot(10)?[4], 7);
        Ok(())
    }
}
