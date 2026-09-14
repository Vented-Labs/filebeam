use std::{
    collections::HashSet,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

use anyhow::Result;
use filebeam_transfer_native::control::{
    Cancelled, Control, MemoryBudget, MemoryExhausted, MemoryPermit,
};

use crate::JobError;

/// Fixed overhead for the worker stack, Tokio drivers, and request bookkeeping.
pub const RUNTIME_ALLOWANCE_BYTES: u64 = 4 * 1024 * 1024;
/// Argon2id needs 64 MiB; manifests may use the same transient headroom.
pub const TRANSIENT_MEMORY_ALLOWANCE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedulerLimits {
    pub workers: usize,
    pub memory_bytes: u64,
}

#[derive(Clone)]
pub struct Scheduler {
    state: Arc<(Mutex<State>, Condvar)>,
    memory: MemoryBudget,
}

struct State {
    workers: usize,
    used_workers: usize,
    active_resumes: HashSet<String>,
}

impl Scheduler {
    pub fn new(limits: SchedulerLimits) -> Result<Self> {
        if limits.workers == 0 || limits.memory_bytes < RUNTIME_ALLOWANCE_BYTES {
            return Err(JobError::resource(
                "scheduler limits must allow at least one worker and its runtime overhead",
            )
            .into());
        }
        Ok(Self {
            state: Arc::new((
                Mutex::new(State {
                    workers: limits.workers,
                    used_workers: 0,
                    active_resumes: HashSet::new(),
                }),
                Condvar::new(),
            )),
            memory: MemoryBudget::new(limits.memory_bytes),
        })
    }

    pub fn admit(&self, control: &Control, resume_id: Option<&str>) -> Result<Admission> {
        let requested = control
            .memory_budget()
            .checked_add(RUNTIME_ALLOWANCE_BYTES)
            .ok_or_else(|| {
                JobError::resource("transfer memory budget overflows scheduler accounting")
            })?;
        let (lock, wake) = &*self.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(id) = resume_id
            && state.active_resumes.contains(id)
        {
            return Err(JobError::resource(format!(
                "saved transfer {id} is still stopping or running"
            ))
            .into());
        }
        while state.used_workers == state.workers {
            if control.cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(Cancelled.into());
            }
            let (next, _) = wake
                .wait_timeout(state, Duration::from_millis(20))
                .unwrap_or_else(|error| error.into_inner());
            state = next;
        }
        control.check()?;
        state.used_workers += 1;
        drop(state);
        let memory = match self.memory.reserve(requested, &control.cancelled) {
            Ok(memory) => memory,
            Err(error) => {
                let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
                state.used_workers -= 1;
                wake.notify_all();
                return if error.is::<MemoryExhausted>() {
                    Err(JobError::resource(format!(
                        "transfer needs {requested} bytes but the shared scheduler budget is exhausted"
                    ))
                    .into())
                } else {
                    Err(error)
                };
            }
        };
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(id) = resume_id
            && state.active_resumes.contains(id)
        {
            state.used_workers -= 1;
            wake.notify_all();
            return Err(JobError::resource(format!(
                "saved transfer {id} is still stopping or running"
            ))
            .into());
        }
        if let Some(id) = resume_id {
            state.active_resumes.insert(id.to_owned());
        }
        drop(state);
        control.set_memory_budget(self.memory.clone());
        Ok(Admission {
            scheduler: self.clone(),
            _memory: memory,
            resume_id: resume_id.map(str::to_owned),
        })
    }
}

pub struct Admission {
    scheduler: Scheduler,
    _memory: MemoryPermit,
    resume_id: Option<String>,
}

impl Drop for Admission {
    fn drop(&mut self) {
        let (lock, wake) = &*self.scheduler.state;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.used_workers -= 1;
        if let Some(id) = &self.resume_id {
            state.active_resumes.remove(id);
        }
        wake.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use filebeam_transfer_native::control::TransferSettings;
    use std::{path::PathBuf, sync::mpsc, thread};

    fn control(memory_budget: u64) -> Control {
        Control::new(
            TransferSettings {
                state_home: PathBuf::from("transfers"),
                max_concurrency: Some(1),
                memory_budget,
                client_user_agent: None,
                webrtc_relay_only: false,
                checkpoint_secret_store: None,
                source_resolver: None,
            },
            mpsc::channel().0,
        )
    }

    #[test]
    fn cancellation_interrupts_memory_admission() {
        let scheduler = Scheduler::new(SchedulerLimits {
            workers: 1,
            memory_bytes: RUNTIME_ALLOWANCE_BYTES + 10,
        })
        .unwrap();
        let first = scheduler.admit(&control(10), None).unwrap();
        let waiting = control(10);
        let task = thread::spawn({
            let scheduler = scheduler.clone();
            let waiting = waiting.clone();
            move || scheduler.admit(&waiting, None).is_err()
        });
        thread::sleep(Duration::from_millis(25));
        waiting.cancel();
        assert!(task.join().unwrap());
        drop(first);
    }

    #[test]
    fn admission_accounts_for_runtime_and_excludes_duplicate_resume() {
        let scheduler = Scheduler::new(SchedulerLimits {
            workers: 2,
            memory_bytes: RUNTIME_ALLOWANCE_BYTES + 10,
        })
        .unwrap();
        let error = match scheduler.admit(&control(11), None) {
            Ok(_) => panic!("oversized admission unexpectedly succeeded"),
            Err(error) => error,
        };
        assert_eq!(
            error.downcast_ref::<JobError>().unwrap().kind,
            crate::JobErrorKind::ResourceExhausted
        );
        let active = scheduler.admit(&control(10), Some("job")).unwrap();
        assert!(scheduler.admit(&control(10), Some("job")).is_err());
        drop(active);
        assert!(scheduler.admit(&control(10), Some("job")).is_ok());
    }

    #[test]
    fn transient_reservations_share_the_scheduler_budget() {
        let scheduler = Scheduler::new(SchedulerLimits {
            workers: 1,
            memory_bytes: RUNTIME_ALLOWANCE_BYTES + 15,
        })
        .unwrap();
        let control = control(10);
        let _admission = scheduler.admit(&control, None).unwrap();
        let _crypto = control.reserve_memory(5).unwrap();
        control.cancel();
        let error = match control.reserve_memory(1) {
            Ok(_) => panic!("cancelled reservation unexpectedly succeeded"),
            Err(error) => error,
        };
        assert!(error.is::<Cancelled>());
    }
}
