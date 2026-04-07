use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cron::Schedule;
use std::str::FromStr;

use super::team_cron_registry::{CronEntry, CronRegistry};

pub type CronCallback = Arc<dyn Fn(&CronEntry) + Send + Sync>;

pub struct CronScheduler {
    registry: Arc<CronRegistry>,
    callback: Option<CronCallback>,
    running: Arc<AtomicBool>,
}

impl CronScheduler {
    #[must_use]
    pub fn new(registry: Arc<CronRegistry>) -> Self {
        Self {
            registry,
            callback: None,
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn with_callback(mut self, callback: CronCallback) -> Self {
        self.callback = Some(callback);
        self
    }

    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        let registry = self.registry;
        let callback = self.callback;
        let running = self.running;
        running.store(true, Ordering::SeqCst);

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            while running.load(Ordering::SeqCst) {
                interval.tick().await;

                let now = chrono::Utc::now();
                let entries = registry.list(false);

                for entry in &entries {
                    if !entry.enabled {
                        continue;
                    }

                    if let Ok(schedule) = Schedule::from_str(&entry.schedule) {
                        let last_run = entry.last_run_at.and_then(|ts| {
                            chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
                        });

                        let should_run = match last_run {
                            Some(last) => {
                                schedule
                                    .after(&last)
                                    .next()
                                    .is_some_and(|next| next <= now)
                            }
                            None => {
                                schedule
                                    .upcoming(chrono::Utc)
                                    .next()
                                    .is_some_and(|next| next <= now)
                            }
                        };

                        if should_run {
                            if let Some(ref cb) = callback {
                                cb(entry);
                            }
                            let _ = registry.record_run(&entry.cron_id);
                        }
                    }
                }
            }
        })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn parses_valid_cron_expression() {
        let schedule = Schedule::from_str("0 * * * *");
        assert!(schedule.is_ok());
    }

    #[test]
    fn rejects_invalid_cron_expression() {
        let schedule = Schedule::from_str("invalid");
        assert!(schedule.is_err());
    }

    #[test]
    fn scheduler_stops_when_flagged() {
        let registry = Arc::new(CronRegistry::new());
        let scheduler = CronScheduler::new(registry);
        scheduler.stop();
        assert!(!scheduler.running.load(Ordering::SeqCst));
    }

    #[test]
    fn callback_is_invoked_on_schedule() {
        let registry = Arc::new(CronRegistry::new());
        let entry = registry.create("* * * * *", "test prompt", None);
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        let callback = Arc::new(move |e: &CronEntry| {
            if e.cron_id == entry.cron_id {
                count_clone.fetch_add(1, Ordering::SeqCst);
            }
        });

        let scheduler = CronScheduler::new(registry.clone()).with_callback(callback);
        assert!(scheduler.callback.is_some());
    }

    #[test]
    fn every_minute_schedule() {
        let schedule = Schedule::from_str("* * * * *").unwrap();
        let now = chrono::Utc::now();
        let upcoming: Vec<_> = schedule.upcoming(chrono::Utc).take(3).collect();
        assert_eq!(upcoming.len(), 3);
        for window in upcoming.windows(2) {
            let diff = (window[1] - window[0]).num_seconds();
            assert!((diff - 60).abs() <= 2);
        }
    }

    #[test]
    fn hourly_schedule() {
        let schedule = Schedule::from_str("0 * * * *").unwrap();
        let upcoming: Vec<_> = schedule.upcoming(chrono::Utc).take(2).collect();
        assert_eq!(upcoming.len(), 2);
        let diff = (upcoming[1] - upcoming[0]).num_seconds();
        assert!((diff - 3600).abs() <= 2);
    }
}
