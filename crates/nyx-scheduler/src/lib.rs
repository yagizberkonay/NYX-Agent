use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SchedulerError {
    #[error("schedule storage error: {0}")]
    Storage(String),
    #[error("schedule input error: {0}")]
    Input(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScheduleStatus {
    Active,
    Paused,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub id: Uuid,
    pub name: String,
    pub tool: String,
    pub input: serde_json::Value,
    pub interval_seconds: u64,
    pub next_run_at: DateTime<Utc>,
    pub status: ScheduleStatus,
    pub last_run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct ScheduleStore {
    path: PathBuf,
}

impl ScheduleStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<Vec<ScheduledJob>, SchedulerError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data =
            fs::read_to_string(&self.path).map_err(|e| SchedulerError::Storage(e.to_string()))?;
        serde_json::from_str(&data).map_err(|e| SchedulerError::Storage(e.to_string()))
    }

    pub fn save(&self, jobs: &[ScheduledJob]) -> Result<(), SchedulerError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|e| SchedulerError::Storage(e.to_string()))?;
        }
        let tmp = self.path.with_extension("tmp");
        let data = serde_json::to_string_pretty(jobs)
            .map_err(|e| SchedulerError::Storage(e.to_string()))?;
        fs::write(&tmp, data).map_err(|e| SchedulerError::Storage(e.to_string()))?;
        fs::rename(&tmp, &self.path).map_err(|e| SchedulerError::Storage(e.to_string()))
    }

    pub fn add(
        &self,
        name: String,
        tool: String,
        input: serde_json::Value,
        interval_seconds: u64,
    ) -> Result<ScheduledJob, SchedulerError> {
        if name.trim().is_empty() || tool.trim().is_empty() {
            return Err(SchedulerError::Input("name and tool are required".into()));
        }
        if interval_seconds < 30 {
            return Err(SchedulerError::Input(
                "interval must be at least 30 seconds".into(),
            ));
        }
        let mut jobs = self.load()?;
        let job = ScheduledJob {
            id: Uuid::new_v4(),
            name,
            tool,
            input,
            interval_seconds,
            next_run_at: Utc::now() + Duration::seconds(interval_seconds as i64),
            status: ScheduleStatus::Active,
            last_run_at: None,
        };
        jobs.push(job.clone());
        self.save(&jobs)?;
        Ok(job)
    }

    pub fn due(&self, now: DateTime<Utc>) -> Result<Vec<ScheduledJob>, SchedulerError> {
        Ok(self
            .load()?
            .into_iter()
            .filter(|job| job.status == ScheduleStatus::Active && job.next_run_at <= now)
            .collect())
    }

    pub fn mark_run(&self, id: Uuid, now: DateTime<Utc>) -> Result<(), SchedulerError> {
        let mut jobs = self.load()?;
        let job = jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or_else(|| SchedulerError::Input("schedule not found".into()))?;
        job.last_run_at = Some(now);
        job.next_run_at = now + Duration::seconds(job.interval_seconds as i64);
        self.save(&jobs)
    }

    pub fn pause(&self, id: Uuid) -> Result<(), SchedulerError> {
        self.set_status(id, ScheduleStatus::Paused)
    }
    pub fn resume(&self, id: Uuid) -> Result<(), SchedulerError> {
        self.set_status(id, ScheduleStatus::Active)
    }

    fn set_status(&self, id: Uuid, status: ScheduleStatus) -> Result<(), SchedulerError> {
        let mut jobs = self.load()?;
        let job = jobs
            .iter_mut()
            .find(|job| job.id == id)
            .ok_or_else(|| SchedulerError::Input("schedule not found".into()))?;
        job.status = status;
        self.save(&jobs)
    }
}

pub fn default_store(root: &Path) -> ScheduleStore {
    ScheduleStore::new(root.join(".nyx").join("schedules.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn persists_and_detects_due_jobs() {
        let path = env::temp_dir().join(format!("nyx-schedules-{}.json", Uuid::new_v4()));
        let store = ScheduleStore::new(&path);
        let job = store
            .add(
                "lead follow-up".into(),
                "crm_search_leads".into(),
                serde_json::json!({"query":"demo"}),
                30,
            )
            .unwrap();
        let mut jobs = store.load().unwrap();
        jobs[0].next_run_at = Utc::now() - Duration::seconds(1);
        store.save(&jobs).unwrap();
        assert_eq!(store.due(Utc::now()).unwrap()[0].id, job.id);
        store.mark_run(job.id, Utc::now()).unwrap();
        assert!(store.due(Utc::now()).unwrap().is_empty());
        let _ = fs::remove_file(path);
    }
}
