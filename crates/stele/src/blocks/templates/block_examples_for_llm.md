# Block Templates for LLM Flow Generation

When generating flows, use these block templates as reference. Replace placeholders (in `{{}}`) with actual values.

## Available Block Types

### Conditional Block

Use for branching logic based on conditions.

```json
{
  "id": "condition_check",
  "type": "Conditional",
  "properties": {
    "condition": "state.user_type == 'premium'",
    "true_block": "premium_content",
    "false_block": "regular_content"
  }
}
```

### Decision Block

Use for multiple choice branching with conditions.

```json
{
  "id": "multi_choice",
  "type": "Decision",
  "properties": {
    "options": [
      { "condition": "state.choice == 'A'", "target": "path_a" },
      { "condition": "state.choice == 'B'", "target": "path_b" }
    ],
    "default_target": "default_path"
  }
}
```

### Display Block

Use for showing messages or content to users.

```json
{
  "id": "show_message",
  "type": "Display",
  "properties": {
    "message": "Welcome to our service!",
    "next_block": "next_step"
  }
}
```

### ExternalData Block

Use for fetching data from external APIs.

```json
{
  "id": "fetch_user_data",
  "type": "ExternalData",
  "properties": {
    "api_url": "https://api.example.com/user/{{user_id}}",
    "data_path": "$.user.profile",
    "next_block": "process_data"
  }
}
```

### GoTo Block

Use for simple navigation to another block.

```json
{
  "id": "navigate_to",
  "type": "GoTo",
  "properties": {
    "target": "destination_block"
  }
}
```

### Input Block

Use for collecting user input.

```json
{
  "id": "get_user_input",
  "type": "Input",
  "properties": {
    "prompt": "Please enter your name:",
    "target": "process_name",
    "input_key": "user_name"
  }
}
```

### Interactive Block

Use for presenting multiple choice options to users.

```json
{
  "id": "user_choice",
  "type": "Interactive",
  "properties": {
    "question": "What would you like to do?",
    "options": [
      { "label": "View Profile", "target": "show_profile" },
      { "label": "Settings", "target": "show_settings" }
    ]
  }
}
```

### Random Block

Use for random selection between options.

```json
{
  "id": "random_content",
  "type": "Random",
  "properties": {
    "options": [
      { "target": "content_a", "weight": 0.3 },
      { "target": "content_b", "weight": 0.7 }
    ]
  }
}
```

### Compute Block

Use for calculations and data processing.

```json
{
  "id": "calculate_total",
  "type": "Compute",
  "properties": {
    "operation": "state.price * state.quantity",
    "result_key": "total_cost",
    "next_block": "show_total"
  }
}
```

## Flow Structure Requirements

Every flow must have:

- `name`: Human-readable flow name
- `start_block_id`: ID of the first block to execute
- `blocks`: Array of block definitions

Example flow structure:

```json
{
  "example_flow": {
    "name": "Example Flow",
    "start_block_id": "start_here",
    "blocks": [
      {
        "id": "start_here",
        "type": "Display",
        "properties": {
          "message": "Flow started",
          "next_block": "end_here"
        }
      },
      {
        "id": "end_here",
        "type": "Display",
        "properties": {
          "message": "Flow completed",
          "next_block": null
        }
      }
    ]
  }
}
```

Copyright (C) 2024 Jonathan Lee.
