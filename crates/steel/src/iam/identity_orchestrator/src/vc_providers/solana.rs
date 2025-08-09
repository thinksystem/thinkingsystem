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

use anchor_client::{anchor_lang::prelude::*, Client, Cluster, Program};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use std::rc::Rc;
use std::str::FromStr;

use iam_program::{accounts as IamAccounts, instruction as IamInstruction, DidAccount};

pub struct SolanaProvider {
    program: Program,
    orchestrator: Rc<Keypair>,
}

impl SolanaProvider {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let rpc_url = "http://127.0.0.1:8899";
        let program_id = Pubkey::from_str("iamPo1fT362b52t2a2eDe4d668b5F4a1E12c8c7")?;

        let orchestrator_path = "./orchestrator-key.json";
        let orchestrator = Rc::new(
            solana_sdk::signature::read_keypair_file(orchestrator_path)
                .expect("Failed to read orchestrator keypair file"),
        );

        let client = Client::new_with_options(
            Cluster::Custom(rpc_url.to_string(), "ws://127.0.0.1:8900".to_string()),
            orchestrator.clone(),
            CommitmentConfig::processed(),
        );

        let program = client.program(program_id)?;

        Ok(Self {
            program,
            orchestrator,
        })
    }

    pub async fn register_did(
        &self,
        user_authority: Pubkey,
        surreal_record_id: String,
    ) -> Result<Pubkey, Box<dyn std::error::Error>> {
        let (did_account_pda, _bump) = Pubkey::find_program_address(
            &[b"did", surreal_record_id.as_bytes()],
            &self.program.id(),
        );

        let builder = self.program.request();
        let sig = builder
            .accounts(IamAccounts::RegisterDid {
                did_account: did_account_pda,
                orchestrator: self.orchestrator.public_key(),
                system_program: solana_sdk::system_program::ID,
            })
            .args(IamInstruction::RegisterDid {
                initial_authority: user_authority,
                surreal_record_id,
            })
            .signer(&*self.orchestrator)
            .send()
            .await?;

        println!("Successfully registered DID on-chain. Signature: {}", sig);
        Ok(did_account_pda)
    }

    pub async fn get_did_account_data(
        &self,
        pda: Pubkey,
    ) -> Result<DidAccount, Box<dyn std::error::Error>> {
        let account: DidAccount = self.program.account(pda).await?;
        Ok(account)
    }
}
