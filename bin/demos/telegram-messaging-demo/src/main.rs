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

mod app;
mod cli;
mod telegram_client;

use app::TelegramApp;
use clap::Parser;
use cli::Args;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

    let args = Args::parse();

    println!("Starting Simple Telegram Demo...");
    if args.get_token().is_some() {
        println!("Bot token loaded from config");
    } else {
        println!("No bot token found. You'll need to enter it in the UI.");
    }

    if args.get_chat_id().is_some() {
        println!("Chat ID loaded from config");
    } else {
        println!("No chat ID found. You'll need to enter it in the UI.");
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_title("Simple Telegram Demo"),
        ..Default::default()
    };

    eframe::run_native(
        "Simple Telegram Demo",
        options,
        Box::new(|_cc| Ok(Box::new(TelegramApp::new(args)))),
    )
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}
