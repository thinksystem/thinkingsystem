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

use super::ledger::Ledger;
use super::shared_types::*;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::mpsc;


pub struct PoPSManager {
    ledger: Ledger,

    node_inboxes: Arc<RwLock<HashMap<NodeID, mpsc::Sender<PoPSMessage>>>>,
}

impl PoPSManager {
    pub fn new(ledger: Ledger) -> Self {
        Self {
            ledger,
            node_inboxes: Arc::new(RwLock::new(HashMap::new())),
        }
    }


    pub fn register_node_inbox(&self, node_id: NodeID, inbox: mpsc::Sender<PoPSMessage>) {
        self.node_inboxes.write().unwrap().insert(node_id, inbox);
    }


    pub async fn execute_staked_task(
        &self,
        task_id: TaskID,
        task_details: String,
        candidate_ids: Vec<NodeID>,
    ) -> Result<String, String> {
        if candidate_ids.is_empty() {
            return Err("No candidate nodes provided.".to_string());
        }


        let announcement = TaskAnnouncement {
            task_id,
            task_details,
        };
        let (bid_tx, mut bid_rx) = mpsc::channel(candidate_ids.len());

        let inboxes = self.node_inboxes.read().unwrap();
        for node_id in &candidate_ids {
            if let Some(inbox) = inboxes.get(node_id) {
                let msg = PoPSMessage::Announce(announcement.clone());
                let bid_tx_clone = bid_tx.clone();
                inbox
                    .send(PoPSMessage::Bid(bid_tx_clone))
                    .await
                    .map_err(|_| format!("Node {node_id} is offline."))?;
            }
        }
        drop(bid_tx);


        let mut bids = Vec::new();
        while let Some(bid) = bid_rx.recv().await {
            bids.push(bid);
        }

        if bids.is_empty() {
            return Err("No performance bids received from candidates.".to_string());
        }


        let best_bid = bids
            .into_iter()
            .max_by_key(|b| b.collateral)
            .ok_or("Failed to select best bid.")?;


        self.ledger
            .stake_collateral(&best_bid)
            .map_err(|e| e.to_string())?;

        let task = StakedTask {
            bid: best_bid,
            client_signature: "client_sig_placeholder".into(),
        };


        let (outcome_tx, outcome_rx) = oneshot::channel();
        if let Some(inbox) = inboxes.get(&task.bid.node_id) {
            inbox
                .send(PoPSMessage::Execute {
                    task: task.clone(),
                    responder: outcome_tx,
                })
                .await
                .map_err(|_| "Failed to send execution command to winning node.".to_string())?;
        } else {
            self.ledger.resolve_failed_stake_and_slash(&task);
            return Err("Winning node went offline before execution.".to_string());
        }

        match tokio::time::timeout(Duration::from_secs(15), outcome_rx).await {
            Ok(Ok(Ok((success_msg, _)))) => {
                self.ledger.resolve_successful_stake(&task);
                Ok(success_msg)
            }
            Ok(Ok(Err(fail_msg))) => {
                self.ledger.resolve_failed_stake_and_slash(&task);
                Err(fail_msg)
            }
            _ => {

                self.ledger.resolve_failed_stake_and_slash(&task);
                Err("Node failed to return execution result in time.".to_string())
            }
        }
    }
}
