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

pub mod algorithms;
pub mod ledger;
pub mod manager;
pub mod shared_types;


pub use algorithms::{
    calculate_consumer_utility, calculate_slash_percentage, calculate_supplier_offering,
    decay_trust_score, update_trust_score_on_failure, update_trust_score_on_success,
};
pub use ledger::Ledger;
pub use manager::PoPSManager;
pub use shared_types::{
    NodeID, PenaltyCurve, PerformanceBid, SlaGuarantees, StakedTask, TaskAnnouncement, TrustScore,
};
