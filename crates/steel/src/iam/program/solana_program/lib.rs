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

use anchor_lang::prelude::*;
use iam_core::{DidDocument, Service as CoreService, VerificationMethod as CoreVerificationMethod};

declare_id!("iamPo1fT362b52t2a2eDe4d668b5F4a1E12c8c7");

#[program]
pub mod iam_program {
    use super::*;

    pub fn register_did(
        ctx: Context<RegisterDid>,
        initial_authority: Pubkey,
        surreal_record_id: String,
    ) -> Result<()> {
        let did_account = &mut ctx.accounts.did_account;
        did_account.authority = initial_authority;
        did_account.orchestrator = ctx.accounts.orchestrator.key();
        did_account.surreal_record_id = surreal_record_id;
        did_account.bump = ctx.bumps.did_account;
        did_account.version = 1;
        let did_uri = format!("did:sol:{initial_authority}");
        let method = VerificationMethod {
            id: format!("{did_uri}#key-1"),
            key_type: "Ed25519VerificationKey2020".to_string(),
            key_data: initial_authority.to_bytes().to_vec(),
            controller: did_uri,
        };
        did_account.verification_methods.push(method);
        did_account.authentication.push(0);
        Ok(())
    }

    pub fn add_verification_method(
        ctx: Context<MutateDid>,
        method: VerificationMethod,
    ) -> Result<()> {
        let did_account = &mut ctx.accounts.did_account;
        require!(
            did_account.verification_methods.len() < 10,
            IamError::MaxVerificationMethodsReached
        );
        did_account.verification_methods.push(method);
        Ok(())
    }

    pub fn add_service(ctx: Context<MutateDid>, service: Service) -> Result<()> {
        let did_account = &mut ctx.accounts.did_account;
        require!(did_account.services.len() < 5, IamError::MaxServicesReached);
        did_account.services.push(service);
        Ok(())
    }

    pub fn create_issuer(ctx: Context<CreateIssuer>) -> Result<()> {
        let issuer_account = &mut ctx.accounts.issuer_config;
        issuer_account.authority = ctx.accounts.authority.key();
        issuer_account.did_account = ctx.accounts.did_account.key();
        issuer_account.bump = ctx.bumps.issuer_config;
        Ok(())
    }

    pub fn issue_credential_status(
        ctx: Context<IssueCredentialStatus>,
        credential_hash: [u8; 32],
    ) -> Result<()> {
        let status_account = &mut ctx.accounts.credential_status;
        status_account.issuer_config = ctx.accounts.issuer_config.key();
        status_account.is_revoked = false;
        status_account.bump = ctx.bumps.credential_status;

        emit!(CredentialIssued {
            issuer: ctx.accounts.authority.key(),
            credential_hash,
        });
        Ok(())
    }

    pub fn revoke_credential_status(
        ctx: Context<RevokeCredentialStatus>,
        _credential_hash: [u8; 32],
    ) -> Result<()> {
        let status_account = &mut ctx.accounts.credential_status;
        status_account.is_revoked = true;
        emit!(CredentialRevoked {
            issuer: ctx.accounts.authority.key(),
            credential_hash: _credential_hash,
        });
        Ok(())
    }
}

#[account]
#[derive(InitSpace)]
pub struct DidAccount {
    pub authority: Pubkey,
    pub orchestrator: Pubkey,
    #[max_len(40)]
    pub surreal_record_id: String,
    pub version: u8,
    pub bump: u8,
    #[max_len(10)]
    pub verification_methods: Vec<VerificationMethod>,
    #[max_len(5)]
    pub services: Vec<Service>,
    #[max_len(10)]
    pub authentication: Vec<u16>,
    #[max_len(10)]
    pub assertion_method: Vec<u16>,
}

#[account]
#[derive(InitSpace)]
pub struct IssuerConfig {
    pub authority: Pubkey,
    pub did_account: Pubkey,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct CredentialStatus {
    pub issuer_config: Pubkey,
    pub is_revoked: bool,
    pub bump: u8,
}

#[derive(Accounts)]
#[instruction(initial_authority: Pubkey, surreal_record_id: String)]
pub struct RegisterDid<'info> {
    #[account(init, payer = orchestrator, space = 8 + DidAccount::INIT_SPACE, seeds = [b"did", surreal_record_id.as_bytes()], bump)]
    pub did_account: Account<'info, DidAccount>,
    #[account(mut)]
    pub orchestrator: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MutateDid<'info> {
    #[account(mut, has_one = authority)]
    pub did_account: Account<'info, DidAccount>,
    pub authority: Signer<'info>,
}

#[derive(Accounts)]
pub struct CreateIssuer<'info> {
    #[account(init, payer = authority, space = 8 + IssuerConfig::INIT_SPACE, seeds = [b"issuer", did_account.key().as_ref()], bump)]
    pub issuer_config: Account<'info, IssuerConfig>,
    #[account(has_one = authority)]
    pub did_account: Account<'info, DidAccount>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(credential_hash: [u8; 32])]
pub struct IssueCredentialStatus<'info> {
    #[account(init, payer = authority, space = 8 + CredentialStatus::INIT_SPACE, seeds = [b"vc_status", issuer_config.key().as_ref(), &credential_hash], bump)]
    pub credential_status: Account<'info, CredentialStatus>,
    #[account(has_one = authority)]
    pub issuer_config: Account<'info, IssuerConfig>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(credential_hash: [u8; 32])]
pub struct RevokeCredentialStatus<'info> {
    #[account(mut, has_one = issuer_config, seeds = [b"vc_status", issuer_config.key().as_ref(), &credential_hash], bump = credential_status.bump)]
    pub credential_status: Account<'info, CredentialStatus>,
    #[account(has_one = authority)]
    pub issuer_config: Account<'info, IssuerConfig>,
    pub authority: Signer<'info>,
}

#[derive(AnchorSerialise, AnchorDeserialise, Clone, InitSpace)]
pub struct VerificationMethod {
    #[max_len(60)]
    pub id: String,
    #[max_len(40)]
    pub key_type: String,
    #[max_len(32)]
    pub key_data: Vec<u8>,
    #[max_len(60)]
    pub controller: String,
}

#[derive(AnchorSerialise, AnchorDeserialise, Clone, InitSpace)]
pub struct Service {
    #[max_len(60)]
    pub id: String,
    #[max_len(40)]
    pub service_type: String,
    #[max_len(120)]
    pub service_endpoint: String,
}

#[event]
pub struct CredentialIssued {
    pub issuer: Pubkey,
    pub credential_hash: [u8; 32],
}

#[event]
pub struct CredentialRevoked {
    pub issuer: Pubkey,
    pub credential_hash: [u8; 32],
}

#[error_code]
pub enum IamError {
    MaxVerificationMethodsReached,
    MaxServicesReached,
}
