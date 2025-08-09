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

use clap::Parser;
use std::env;

#[derive(Parser, Debug, Clone)]
#[command(name = "telegram-messaging-demo")]
#[command(about = "A bi-directional Telegram messaging interface demo")]
pub struct Args {
    #[arg(short, long)]
    pub token: Option<String>,

    #[arg(short, long)]
    pub chat_id: Option<String>,

    #[arg(short, long)]
    pub debug: bool,
}

impl Args {
    pub fn get_token(&self) -> Option<String> {
        self.token
            .clone()
            .or_else(|| env::var("TELEGRAM_BOT_TOKEN").ok())
    }

    pub fn get_chat_id(&self) -> Option<String> {
        self.chat_id
            .clone()
            .or_else(|| env::var("TELEGRAM_CHAT_ID").ok())
    }
}
