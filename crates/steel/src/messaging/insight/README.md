# GLiNER Integration for Steel Insight Module

This document describes the integration of GLiNER (Generic Named Entity Recognition) into the Steel messaging insight module, providing hybrid analysis capabilities that combine syntactic pattern detection with deep learning-based NER.

## Overview

The GLiNER integration adds Named Entity Recognition capabilities to the existing syntactic analysis system, creating a hybrid approach that can:

1. **Detect entities** like persons, emails, phone numbers, credit cards, SSNs, etc.
2. **Combine scores** from both syntactic patterns and NER confidence
3. **Boost detection** for high-confidence NER entities
4. **Provide fallback** to syntactic-only analysis when NER is unavailable

## Architecture

### Core Components

1. **`NerAnalyser`** - Wraps the GLiNER model and provides entity detection
2. **`HybridContentAnalyser`** - Combines syntactic and NER analysis
3. **`MessageSecurity`** - Main service with hybrid analysis support
4. **`ScribeSecurityBridge`** - Bridge service supporting multiple analysis modes

### Analysis Modes

- **Syntactic Only** - Traditional pattern-based detection
- **Hybrid** - Combines syntactic patterns with NER entities
- **LLM Enhanced** - Uses LLM for grey-area analysis (existing)

## Configuration

### NER Configuration (`NerConfig`)

```rust
pub struct NerConfig {
    pub model_path: String,              // Path to GLiNER model (default: "models/gliner-x-small")
    pub enabled: bool,                   // Enable/disable NER
    pub entity_labels: Vec<String>,      // Entity types to detect
    pub min_confidence_threshold: f64,   // Minimum confidence for entities
    pub entity_weights: HashMap<String, f64>, // Risk weights per entity type
    pub max_text_length: usize,          // Maximum text length to process
}
```

### Hybrid Configuration (`HybridConfig`)

```rust
pub struct HybridConfig {
    pub syntactic_weight: f64,        // Weight for syntactic score (default: 0.6)
    pub ner_weight: f64,              // Weight for NER score (default: 0.4)
    pub ner_boost_threshold: f64,     // Threshold for NER boost activation (default: 0.8)
    pub min_combined_threshold: f64,  // Minimum combined score for review (default: 0.5)
    pub enable_ner_boost: bool,       // Enable NER confidence boosting (default: true)
}
```

### TOML Configuration Example

```toml
[messaging.security.ner]
model_path = "models/gliner-x-small"
enabled = true
min_confidence_threshold = 0.5
max_text_length = 2048
entity_labels = ["person", "email", "phone", "credit_card", "ssn"]

[messaging.security.ner.entity_weights]
person = 0.8
email = 0.9
phone = 0.9
credit_card = 1.0
ssn = 1.0
```

> **Note:** The default config file is at `config/messaging/security_config.toml`. The default GLiNER model path is `models/gliner-x-small`. If the model is missing, restore it from backup or re-download as per the Telegram demo README.

## Usage Examples

### 1. Message Security with Hybrid Analysis

```rust
use steel::messaging::insight::{MessageSecurity, NerConfig, HybridConfig};
use std::path::Path;

// Create message security instance
let mut security = MessageSecurity::new(Path::new("./state"));

// Enable hybrid analysis
security.enable_hybrid_analysis(
    Some(NerConfig::default()),
    Some(HybridConfig::default())
)?;

// Analyse content
let result = security.assess_message_risk("Contact john@example.com");
match result {
    MessageAnalysisResult::Hybrid(analysis) => {
        println!("Combined score: {:.3}", analysis.combined_risk_score);
        println!("NER entities: {}", analysis.ner_analysis.entities.len());
        println!("Requires review: {}", analysis.requires_scribes_review);
    }
    MessageAnalysisResult::Syntactic(analysis) => {
        // Fallback when NER unavailable
        println!("Syntactic score: {:.3}", analysis.overall_risk_score);
    }
}
```

### 2. Scribe Bridge with Different Modes

```rust
use steel::messaging::insight::{ScribeSecurityBridge, BridgeMode, ScribeSecurityEvent};

// Create bridge with hybrid mode
let mut bridge = ScribeSecurityBridge::new(
    Path::new("./state"),
    BridgeMode::Hybrid
);

// Process analysis event
let event = ScribeSecurityEvent::AnalyseContent {
    content: "Call 555-123-4567 for support".to_string(),
};

let response = bridge.process_event(event);
```

### 3. Direct Hybrid Analyser Usage

```rust
use steel::messaging::insight::{HybridContentAnalyser, SecurityConfig};

let security_config = SecurityConfig::load_or_default();
let mut analyser = HybridContentAnalyser::from_security_config(&security_config);

// Initialise NER model
analyser.initialise_ner()?;

let mut distribution = ScoreDistribution::default();
let analysis = analyser.analyse_hybrid("Send to alice@company.com", &mut distribution)?;

// Get prioritised entities from both syntactic and NER analysis
let entities = analyser.get_prioritised_entities(&analysis);
for entity in entities {
    println!("{}: {} (risk: {:.3})", entity.source, entity.text, entity.risk_score);
}
```

## GLiNER Model Setup

### 1. Download GLiNER Model

```bash
# Create model directory (default expected by code)
mkdir -p models/gliner-x-small

# Download model files (example URLs - use actual GLiNER model sources)
# - tokenizer.json
# - onnx/model.onnx
# - config.json (if required)
```

### 2. Dependencies

These dependencies are already included in the `steel` crate. You don’t need to add them when using `steel` as a library. Only add them if you’re building a standalone tool.

### 3. Model Files Structure

```
models/gliner-x-small/
├── tokenizer.json
├── onnx/
│   └── model.onnx
└── config.json (optional)
```

## Score Combination Logic

The hybrid analyser combines syntactic and NER scores using:

1. **Weighted Average**: `combined = syntactic_weight * syntactic_score + ner_weight * ner_score`

2. **NER Boost**: When NER confidence is high (above threshold), applies additional boost:

   ```
   boosted_score = min(1.0, combined_score + ner_score * 0.3)
   ```

3. **High-Risk Entity Override**: Certain entities (credit_card, ssn, email, phone) with high confidence automatically trigger review regardless of combined score.

## Entity Types and Weights

Default entity types and their risk weights:

| Entity Type    | Weight | Description             |
| -------------- | ------ | ----------------------- |
| `credit_card`  | 1.0    | Credit card numbers     |
| `ssn`          | 1.0    | Social Security Numbers |
| `email`        | 0.9    | Email addresses         |
| `phone`        | 0.9    | Phone numbers           |
| `person`       | 0.8    | Person names            |
| `ip_address`   | 0.7    | IP addresses            |
| `organisation` | 0.6    | Organisation names      |
| `location`     | 0.5    | Geographic locations    |
| `url`          | 0.4    | URLs                    |
| `date`         | 0.3    | Dates                   |

## Performance Considerations

1. **Model Loading**: GLiNER model is loaded lazily on first use
2. **Text Length**: Long texts are truncated to `max_text_length` (default: 2048 chars)
3. **Fallback**: If NER fails, system falls back to syntactic analysis
4. **Confidence Filtering**: Only entities above `min_confidence_threshold` are used

## Error Handling

The system gracefully handles:

- **Missing model files**: Falls back to syntactic analysis
- **NER inference errors**: Continues with syntactic analysis
- **Model loading failures**: Logs warning and disables NER

## Testing

Run tests for the `steel` crate:

```bash
cargo test -p steel
```

## Future Enhancements

1. **Dynamic Model Loading**: Support for switching models at runtime
2. **Custom Entity Types**: Allow user-defined entity types and patterns
3. **Ensemble Scoring**: Multiple NER models with ensemble scoring
4. **Performance Monitoring**: Track NER inference times and accuracy
5. **Model Fine-tuning**: Integration with feedback loop for model improvement

## Troubleshooting

### GLiNER Model Not Found

```
Error: GLiNER model path does not exist
```

**Solution**: Download and place GLiNER model files in the configured path.

### NER Initialisation Failed

```
Warning: Failed to initialise NER model: ...
```

**Solution**: Check model file integrity and ONNX runtime installation.

### Low NER Performance

**Solutions**:

- Adjust `min_confidence_threshold`
- Modify entity weights
- Update entity labels list
- Check model compatibility

### Memory Issues

**Solutions**:

- Reduce `max_text_length`
- Use smaller GLiNER model variant
- Implement text chunking for long documents

Copyright (C) 2024 Jonathan Lee.
