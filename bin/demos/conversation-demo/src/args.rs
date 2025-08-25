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



#[derive(Clone, Default)]
pub struct Args {
    pub token: Option<String>,
    pub chat_id: Option<String>,
}

impl Args {
    pub fn from_env() -> Self {
        Self {
            token: std::env::var("TELEGRAM_BOT_TOKEN").ok(),
            chat_id: std::env::var("TELEGRAM_CHAT_ID").ok(),
        }
    }
    pub fn get_token(&self) -> Option<String> {
        self.token.clone()
    }
    pub fn get_chat_id(&self) -> Option<String> {
        self.chat_id.clone()
    }
}


impl std::fmt::Debug for Args {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let masked = self
            .token
            .as_deref()
            .map(|t| {
                if t.len() <= 6 {
                    "***".to_string()
                } else {
                    format!("{}…{}", &t[..3], &t[t.len() - 3..])
                }
            })
            .unwrap_or_else(|| "<none>".into());
        f.debug_struct("Args")
            .field("token", &masked)
            .field("chat_id", &self.chat_id)
            .finish()
    }
}
