# Telegram Messaging Demo

A bi-directional Telegram messaging interface built with egui and Rust.

## Features

## Features

- **Bi-directional messaging**: Send and receive messages through the Telegram Bot API
- **Automatic chat discovery**: Automatically discovers and lists available chats when messages are received
- **Smart chat selection**: Select from discovered chats via dropdown menu
- **Real-time updates**: Automatically polls for new messages every 2 seconds
- **Intuitive GUI**: Clean egui-based interface featuring:

  - Connection panel for bot token configuration
  - Chat selection dropdown (auto-populated)
  - Message input area with send button
  - Scrollable message history with sent/received message distinction
  - Status indicators and error reporting

- **Flexible configuration**:

  - Command line arguments for token and default chat ID
  - Environment variable support (via .env file)
  - GUI-based configuration

- **Robust architecture**:
  - Async background tasks for API operations
  - Thread-safe message sharing between GUI and background tasks
  - Proper error handling and connection management
  - Message history management (keeps last 100 messages)

## Prerequisites

1. **Telegram Bot Token**: You need to create a Telegram bot and get its API token:

   - Open Telegram and search for `@BotFather`
   - Send `/newbot` command and follow the instructions
   - Copy the bot token provided by BotFather

2. **Chat ID**: To send messages, you need a chat ID:
   - Add your bot to a chat or group
   - Send a message to the bot
   - Use the Telegram API to get updates: `https://api.telegram.org/bot<YOUR_BOT_TOKEN>/getUpdates`
   - Look for the `chat.id` field in the response

## Usage

### Environment Variables (Recommended)

1. Copy the example environment file:

```bash
cp .env.example .env
```

2. Edit `.env` and add your bot token:

```bash
TELEGRAM_BOT_TOKEN=your_bot_token_here
TELEGRAM_CHAT_ID=your_chat_id_here  # Optional
```

3. Run the application:

```bash
cargo run --bin telegram-messaging-demo
```

### Command Line Arguments

```bash
cargo run --bin telegram-messaging-demo -- --token "your_bot_token_here" --chat-id "your_chat_id"
```

### GUI Configuration

1. Run the application without arguments
2. Enter your bot token in the "Bot Token" field
3. Click "Connect" to establish connection with Telegram API
4. Enter the chat ID where you want to send messages
5. Start messaging!

## Configuration Options

- `--token, -t`: Telegram Bot API token
- `--chat-id, -c`: Default chat ID for sending messages
- `--debug, -d`: Enable debug logging
- `TELEGRAM_BOT_TOKEN`: Environment variable for bot token (recommended)
- `TELEGRAM_CHAT_ID`: Environment variable for default chat ID

**Priority Order**: Command line arguments override environment variables, which override GUI configuration.

## Architecture

The demo consists of several modules:

- **`main.rs`**: Entry point and egui setup
- **`app.rs`**: Main application logic and UI rendering
- **`telegram_client.rs`**: Telegram Bot API client implementation
- **`cli.rs`**: Command-line argument parsing

### Key Components

1. **TelegramClient**: Handles all Telegram API operations

   - Sending messages
   - Polling for updates
   - Connection testing

2. **TelegramApp**: Main application state and UI

   - Message history management
   - Real-time UI updates
   - Background task coordination

3. **Async Architecture**: Uses tokio for async operations while maintaining a responsive egui interface

## Message Flow

1. **Outgoing Messages**:

   - User types message in UI
   - Message sent via tokio channel to background task
   - Background task uses TelegramClient to send via API
   - Sent message added to local message history

2. **Incoming Messages**:
   - Background task polls Telegram API every 2 seconds
   - New messages parsed and added to message history
   - UI automatically updates to show new messages

## Error Handling

- Connection failures are reported in the status area
- Invalid chat IDs are validated before sending
- API errors are logged and displayed to the user
- Network timeouts are handled gracefully

## Security Notes

- Bot tokens are masked in the UI (password field)
- Tokens can be provided via environment variables
- No sensitive data is logged in release builds

## Dependencies

- `egui` & `eframe`: GUI framework
- `tokio`: Async runtime
- `reqwest`: HTTP client for Telegram API
- `serde` & `serde_json`: Serialization
- `chrono`: Date/time handling
- `tracing`: Logging
- `anyhow`: Error handling
- Local crates: `stele`, `steel`

## GLiNER ONNX model setup (do not commit models)

This demo can run a local Named Entity Recognition (NER) pass using a GLiNER ONNX model. Models are large and should not be pushed to git (the root `.gitignore` already ignores `/models/`).

1. Create a local models directory:

```bash
mkdir -p models
```

2. Download or copy a GLiNER model folder into `models/`. For example, place the directory at:

```
models/gliner_small-v2.1/
```

Your model directory should contain the ONNX file(s) and tokenizer config used by your GLiNER build.

3. Point the messaging security config to your model path by editing:

`config/messaging/hybrid_security_config.toml`

Set the following keys under `[messaging.security.ner]`:

```toml
[messaging.security.ner]
model_path = "models/gliner_small-v2.1"
enabled = true
min_confidence_threshold = 0.5
```

4. Run the demo as usual; the model will be loaded from your local filesystem. Nothing is uploaded to remote providers.

Note: You can choose any GLiNER-compatible ONNX model and path; just update `model_path` accordingly. Keep the models directory out of git to avoid large binary churn.

Copyright (C) 2024 Jonathan Lee.
