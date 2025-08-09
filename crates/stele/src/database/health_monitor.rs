// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

use crate::database::data_interpreter::{DatabaseInterface, DatabaseTaskError};
use std::time::{Duration, Instant};
use tokio::signal::unix::{signal, SignalKind};
use tracing::{error, info, warn};
#[derive(Debug, thiserror::Error)]
pub enum MonitorError {
    #[error("Failed to set up OS signal handling: {0}")]
    SignalSetupFailed(#[from] std::io::Error),
    #[error("Database health checks failed consecutively, reaching the limit of {max_failures}: {source}")]
    MaxFailuresReached {
        max_failures: u32,
        #[source]
        source: DatabaseTaskError,
    },
    #[error("The health monitor was shut down gracefully by a signal.")]
    ShutdownSignalReceived,
    #[error("Failed during shutdown: {0}")]
    ShutdownFailed(#[source] Box<dyn std::error::Error>),
}
pub struct DatabaseHealthMonitor {
    interface: DatabaseInterface,
    health_check_interval: Duration,
    max_consecutive_failures: u32,
    consecutive_failures: u32,
    last_success: Option<Instant>,
}
impl DatabaseHealthMonitor {
    pub fn new(interface: DatabaseInterface) -> Self {
        Self {
            interface,
            health_check_interval: Duration::from_secs(30),
            max_consecutive_failures: 3,
            consecutive_failures: 0,
            last_success: None,
        }
    }
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.health_check_interval = interval;
        self
    }
    pub fn with_max_failures(mut self, max_failures: u32) -> Self {
        self.max_consecutive_failures = max_failures;
        self
    }
    pub async fn start_monitoring(mut self) -> MonitorError {
        info!(
            interval = ?self.health_check_interval,
            max_failures = self.max_consecutive_failures,
            "Starting database health monitoring."
        );
        let mut interval = tokio::time::interval(self.health_check_interval);
        let mut sigterm = match signal(SignalKind::terminate()) {
            Ok(s) => s,
            Err(e) => return MonitorError::SignalSetupFailed(e),
        };
        let mut sigint = match signal(SignalKind::interrupt()) {
            Ok(s) => s,
            Err(e) => return MonitorError::SignalSetupFailed(e),
        };
        loop {
            let signal_name = tokio::select! {
                _ = interval.tick() => {
                    match self.interface.check_database_health().await {
                        Ok(()) => {
                            if self.consecutive_failures > 0 {
                                info!("Database connection recovered.");
                            }
                            self.consecutive_failures = 0;
                            self.last_success = Some(Instant::now());
                        }
                        Err(e) => {
                            self.consecutive_failures += 1;
                            warn!(
                                attempt = self.consecutive_failures,
                                max_attempts = self.max_consecutive_failures,
                                error = %e,
                                "Health check failed."
                            );
                            if self.consecutive_failures >= self.max_consecutive_failures {
                                error!("Maximum consecutive failures reached. Initiating shutdown.");
                                if let Err(shutdown_err) = self.graceful_shutdown().await {
                                    return shutdown_err;
                                }
                                return MonitorError::MaxFailuresReached {
                                    max_failures: self.max_consecutive_failures,
                                    source: DatabaseTaskError::TaskFinished(e),
                                };
                            }
                        }
                    }
                    continue;
                }
                _ = sigterm.recv() => "SIGTERM",
                _ = sigint.recv() => "SIGINT",
            };
            info!(
                signal = signal_name,
                "Received signal, shutting down health monitor gracefully..."
            );
            if let Err(e) = self.graceful_shutdown().await {
                return e;
            }
            return MonitorError::ShutdownSignalReceived;
        }
    }
    async fn graceful_shutdown(&mut self) -> Result<(), MonitorError> {
        info!("Initiating graceful shutdown of database interface...");
        match self.interface.shutdown().await {
            Ok(()) => {
                info!("✓ Database interface shut down successfully.");
                Ok(())
            }
            Err(e) => {
                error!(error = %e, "✗ Error during graceful shutdown.");
                Err(MonitorError::ShutdownFailed(e.into()))
            }
        }
    }
    pub fn get_health_summary(&self) -> HealthSummary {
        HealthSummary {
            task_alive: self.interface.is_database_task_alive(),
            consecutive_failures: self.consecutive_failures,
            last_success: self.last_success,
            max_consecutive_failures: self.max_consecutive_failures,
        }
    }
}
#[derive(Debug)]
pub struct HealthSummary {
    pub task_alive: bool,
    pub consecutive_failures: u32,
    pub last_success: Option<Instant>,
    pub max_consecutive_failures: u32,
}
impl HealthSummary {
    pub fn is_healthy(&self) -> bool {
        self.task_alive && self.consecutive_failures == 0
    }
    pub fn is_warning(&self) -> bool {
        self.task_alive
            && self.consecutive_failures > 0
            && self.consecutive_failures < self.max_consecutive_failures
    }
    pub fn is_critical(&self) -> bool {
        !self.task_alive || self.consecutive_failures >= self.max_consecutive_failures
    }
    pub fn status_string(&self) -> &'static str {
        if self.is_healthy() {
            "Healthy"
        } else if self.is_warning() {
            "Warning"
        } else {
            "Critical"
        }
    }
}
impl std::fmt::Display for HealthSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Database Health Summary:")?;
        writeln!(f, "  - Status: {}", self.status_string())?;
        writeln!(f, "  - Task Alive: {}", self.task_alive)?;
        writeln!(
            f,
            "  - Consecutive Failures: {}/{}",
            self.consecutive_failures, self.max_consecutive_failures
        )?;
        match self.last_success {
            Some(time) => writeln!(f, "  - Last Success: {:.2?} ago", time.elapsed())?,
            None => writeln!(f, "  - Last Success: Never")?,
        }
        Ok(())
    }
}
pub async fn run_monitored_database_interface() -> Result<(), Box<dyn std::error::Error>> {
    let interface = DatabaseInterface::new().await?;
    let monitor = DatabaseHealthMonitor::new(interface)
        .with_interval(Duration::from_secs(15))
        .with_max_failures(5);
    match monitor.start_monitoring().await {
        MonitorError::ShutdownSignalReceived => {
            info!("Shutdown complete.");
            Ok(())
        }
        e => {
            error!(error = %e, "Health monitor exited with a critical error.");
            Err(e.into())
        }
    }
}
