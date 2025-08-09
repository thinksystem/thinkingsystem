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

use super::shared_types::{NodeID, PerformanceBid, StakedTask, TaskID};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};


#[derive(Debug, Clone, PartialEq)]
pub enum StakeStatus {

    Locked { amount: u64 },

    ResolvedSuccess { returned: u64, paid: u64 },

    ResolvedSlashed { forfeited: u64 },
}


#[derive(Debug, Clone)]
pub struct StakeRecord {
    pub task_id: TaskID,
    pub node_id: NodeID,
    pub status: StakeStatus,
    pub timestamp: u64,
}


#[derive(Debug, Default)]
pub struct LedgerState {

    balances: HashMap<NodeID, u64>,

    stake_log: HashMap<TaskID, Vec<StakeRecord>>,
}


#[derive(Clone, Debug)]
pub struct Ledger(Arc<Mutex<LedgerState>>);

impl Ledger {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(LedgerState::default())))
    }


    fn new_record(&self, task_id: TaskID, node_id: NodeID, status: StakeStatus) -> StakeRecord {
        StakeRecord {
            task_id,
            node_id,
            status,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        }
    }


    pub fn stake_collateral(&self, bid: &PerformanceBid) -> Result<(), &'static str> {
        let mut state = self.0.lock().unwrap();
        let balance = state
            .balances
            .get_mut(&bid.node_id)
            .ok_or("Node not found in ledger")?;

        if *balance < bid.collateral {
            return Err("Insufficient funds for collateral");
        }
        *balance -= bid.collateral;

        let record = self.new_record(
            bid.task_id,
            bid.node_id.clone(),
            StakeStatus::Locked {
                amount: bid.collateral,
            },
        );

        state.stake_log.entry(bid.task_id).or_default().push(record);
        Ok(())
    }


    pub fn resolve_successful_stake(&self, task: &StakedTask) {
        let mut state = self.0.lock().unwrap();
        if let Some(balance) = state.balances.get_mut(&task.bid.node_id) {
            *balance += task.bid.collateral;
            *balance += task.bid.reward;

            let record = self.new_record(
                task.bid.task_id,
                task.bid.node_id.clone(),
                StakeStatus::ResolvedSuccess {
                    returned: task.bid.collateral,
                    paid: task.bid.reward,
                },
            );
            state
                .stake_log
                .entry(task.bid.task_id)
                .or_default()
                .push(record);
        }
    }


    pub fn resolve_failed_stake_and_slash(&self, task: &StakedTask) {
        let mut state = self.0.lock().unwrap();


        let record = self.new_record(
            task.bid.task_id,
            task.bid.node_id.clone(),
            StakeStatus::ResolvedSlashed {
                forfeited: task.bid.collateral,
            },
        );
        state
            .stake_log
            .entry(task.bid.task_id)
            .or_default()
            .push(record);
    }


    pub fn get_balance(&self, node_id: &str) -> u64 {
        let state = self.0.lock().unwrap();
        *state.balances.get(node_id).unwrap_or(&0)
    }

    pub fn set_balance(&self, node_id: &str, balance: u64) {
        let mut state = self.0.lock().unwrap();
        state.balances.insert(node_id.to_string(), balance);
    }

    pub fn get_task_history(&self, task_id: &TaskID) -> Option<Vec<StakeRecord>> {
        let state = self.0.lock().unwrap();
        state.stake_log.get(task_id).cloned()
    }
}
