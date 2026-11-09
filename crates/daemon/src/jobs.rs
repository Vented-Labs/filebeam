use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use uuid::Uuid;

use crate::{DeviceJob, DeviceJobState, daemon_store};

pub struct DeviceJobs {
    home: PathBuf,
    jobs: Vec<DeviceJob>,
}

impl DeviceJobs {
    pub fn open(home: &Path) -> Result<Self> {
        let store = daemon_store(home)?;
        let mut jobs: Vec<DeviceJob> = store.load_named("device-jobs-v1.json")?.unwrap_or_default();
        // A native worker cannot survive a daemon restart. Prepared artifacts are
        // immutable and safe to queue again; incomplete preparation must start
        // over, which creates a new transfer key and nonce material.
        let mut recovered = false;
        for job in &mut jobs {
            match job.state {
                DeviceJobState::Preparing => {
                    let checkpoint_root = home.join("transfers");
                    if checkpoint_root.join(job.id.to_string()).exists() {
                        // Incomplete ciphertext must never be reused with a
                        // restarted preparation pass and its fresh key material.
                        filebeam_transfer_native::checkpoint::Store::discard(
                            &checkpoint_root,
                            &job.id.to_string(),
                        )?;
                    }
                    job.state = DeviceJobState::AwaitingPeerConfiguration;
                    job.prepared = false;
                    recovered = true;
                }
                DeviceJobState::Connecting | DeviceJobState::Transferring if job.prepared => {
                    job.state = DeviceJobState::AwaitingPeerConfiguration;
                    recovered = true;
                }
                DeviceJobState::Connecting | DeviceJobState::Transferring => {
                    job.state = DeviceJobState::AwaitingPeerConfiguration;
                    job.prepared = false;
                    recovered = true;
                }
                _ => {}
            }
        }
        let result = Self {
            home: home.into(),
            jobs,
        };
        if recovered {
            result.persist()?;
        }
        Ok(result)
    }
    pub fn list(&self) -> &[DeviceJob] {
        &self.jobs
    }
    pub fn pending(&self) -> Vec<DeviceJob> {
        self.jobs
            .iter()
            .filter(|job| matches!(job.state, DeviceJobState::AwaitingPeerConfiguration))
            .cloned()
            .collect()
    }
    pub fn enqueue(
        &mut self,
        target: &str,
        grant_id: &str,
        sources: Vec<PathBuf>,
    ) -> Result<DeviceJob> {
        const MAX_QUEUED_JOBS: usize = 32;
        if self
            .jobs
            .iter()
            .filter(|job| {
                !matches!(
                    job.state,
                    DeviceJobState::Delivered | DeviceJobState::Failed | DeviceJobState::Cancelled
                )
            })
            .count()
            >= MAX_QUEUED_JOBS
        {
            bail!("device job queue is full");
        }
        if target.is_empty() || target.len() > 128 || target.contains(['/', '\\', '\0']) {
            bail!("device target is invalid");
        }
        if daemon_store(&self.home)?
            .load_named::<crate::DeviceProvision>("device-provision-v1.json")?
            .is_some_and(|provision| provision.device_id == target)
        {
            bail!("cannot send a device job to this device");
        }
        if sources.is_empty() {
            bail!("provide at least one source path");
        }
        if grant_id.is_empty() || grant_id.len() > 80 || grant_id.contains(['/', '\\', '\0']) {
            bail!("destination grant must be an alias or UUID");
        }
        let sources = sources
            .into_iter()
            .map(|path| {
                let metadata = fs::symlink_metadata(&path)
                    .with_context(|| format!("inspect source {}", path.display()))?;
                if metadata.file_type().is_symlink() || !(metadata.is_file() || metadata.is_dir()) {
                    bail!("source must be a regular file or directory");
                }
                fs::canonicalize(path).context("resolve source")
            })
            .collect::<Result<Vec<_>>>()?;
        let job = DeviceJob {
            id: Uuid::new_v4(),
            target: target.into(),
            grant_id: grant_id.into(),
            sources,
            state: DeviceJobState::AwaitingPeerConfiguration,
            prepared: false,
            error: None,
            expires_at_unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .context("system clock is before Unix epoch")?
                .as_secs()
                .saturating_add(7 * 24 * 60 * 60),
        };
        self.jobs.push(job.clone());
        self.persist()?;
        Ok(job)
    }
    pub fn control(&mut self, id: Uuid, action: DeviceJobState) -> Result<DeviceJob> {
        self.validate_control(id, &action)?;
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .context("device job was not found")?;
        job.state = action;
        let result = job.clone();
        self.persist()?;
        Ok(result)
    }
    pub fn validate_control(&self, id: Uuid, action: &DeviceJobState) -> Result<()> {
        let job = self
            .jobs
            .iter()
            .find(|job| job.id == id)
            .context("device job was not found")?;
        if job.state == DeviceJobState::Cancelled {
            bail!("cancelled device jobs cannot be resumed");
        }
        if !matches!(
            action,
            DeviceJobState::Paused
                | DeviceJobState::AwaitingPeerConfiguration
                | DeviceJobState::Cancelled
        ) {
            bail!("invalid device job control action");
        }
        if matches!(
            job.state,
            DeviceJobState::Delivered | DeviceJobState::Failed
        ) {
            bail!("terminal device jobs cannot be controlled");
        }
        if *action == DeviceJobState::AwaitingPeerConfiguration
            && job.state != DeviceJobState::Paused
        {
            bail!("only paused device jobs can be resumed");
        }
        Ok(())
    }
    pub fn set_state(&mut self, id: Uuid, state: DeviceJobState, prepared: bool) -> Result<()> {
        let job = self
            .jobs
            .iter_mut()
            .find(|job| job.id == id)
            .context("device job was not found")?;
        if matches!(
            job.state,
            DeviceJobState::Paused | DeviceJobState::Cancelled
        ) && job.state != state
        {
            bail!("paused or cancelled device jobs cannot be overwritten by a worker");
        }
        job.state = state;
        job.prepared = prepared;
        if job.state != DeviceJobState::Failed {
            job.error = None;
        }
        self.persist()
    }
    /// Marks a job failed with a reason, unless a user paused or cancelled it.
    pub fn fail(&mut self, id: Uuid, reason: &str) -> Result<()> {
        self.set_state(id, DeviceJobState::Failed, false)?;
        if let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) {
            job.error = Some(reason.into());
        }
        self.persist()
    }
    /// Records why a queued job is still waiting without changing its state.
    pub fn note_retry(&mut self, id: Uuid, reason: &str) -> Result<()> {
        let Some(job) = self.jobs.iter_mut().find(|job| job.id == id) else {
            return Ok(());
        };
        if job.error.as_deref() == Some(reason) {
            return Ok(());
        }
        job.error = Some(reason.into());
        self.persist()
    }
    /// Fails queued jobs whose delivery window has passed.
    pub fn expire(&mut self, now_unix_seconds: u64) -> Result<()> {
        let mut changed = false;
        for job in &mut self.jobs {
            if job.state == DeviceJobState::AwaitingPeerConfiguration
                && job.expires_at_unix_seconds != 0
                && job.expires_at_unix_seconds <= now_unix_seconds
            {
                job.state = DeviceJobState::Failed;
                job.error = Some("the job expired before it was delivered".into());
                changed = true;
            }
        }
        if changed { self.persist() } else { Ok(()) }
    }
    fn persist(&self) -> Result<()> {
        daemon_store(&self.home)?.save_named("device-jobs-v1.json", &self.jobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_updates_do_not_replace_paused_or_cancelled_jobs() {
        let home = tempfile::tempdir().unwrap();
        let id = Uuid::new_v4();
        let mut jobs = DeviceJobs {
            home: home.path().into(),
            jobs: vec![DeviceJob {
                id,
                target: "peer".into(),
                grant_id: "grant".into(),
                sources: Vec::new(),
                state: DeviceJobState::Paused,
                prepared: true,
                expires_at_unix_seconds: 0,
                error: None,
            }],
        };
        assert!(jobs.set_state(id, DeviceJobState::Delivered, true).is_err());
        jobs.jobs[0].state = DeviceJobState::Cancelled;
        assert!(jobs.set_state(id, DeviceJobState::Failed, true).is_err());
    }

    #[test]
    fn expired_queued_jobs_fail_with_a_reason() {
        let home = tempfile::tempdir().unwrap();
        let queued = Uuid::new_v4();
        let paused = Uuid::new_v4();
        let job = |id, state| DeviceJob {
            id,
            target: "peer".into(),
            grant_id: "dropzone".into(),
            sources: Vec::new(),
            state,
            prepared: true,
            expires_at_unix_seconds: 10,
            error: None,
        };
        let mut jobs = DeviceJobs {
            home: home.path().into(),
            jobs: vec![
                job(queued, DeviceJobState::AwaitingPeerConfiguration),
                job(paused, DeviceJobState::Paused),
            ],
        };
        jobs.expire(9).unwrap();
        assert!(jobs.list().iter().all(|job| job.error.is_none()));
        jobs.expire(10).unwrap();
        assert_eq!(jobs.list()[0].state, DeviceJobState::Failed);
        assert!(jobs.list()[0].error.is_some());
        assert_eq!(jobs.list()[1].state, DeviceJobState::Paused);
        assert!(jobs.pending().is_empty());
    }
}
