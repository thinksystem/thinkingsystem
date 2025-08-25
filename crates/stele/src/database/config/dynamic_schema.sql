-- SPDX-License-Identifier: AGPL-3.0-only
-- Dynamic (NLU-friendly) graph schema (mostly schemaless)

DEFINE ANALYZER ascii TOKENIZERS class FILTERS lowercase, ascii;

-- Sources and utterances
DEFINE TABLE source SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD user_id ON TABLE source TYPE string;
DEFINE FIELD channel ON TABLE source TYPE string;
DEFINE FIELD properties ON TABLE source TYPE object;
DEFINE FIELD created_at ON TABLE source TYPE datetime VALUE time::now();

DEFINE TABLE utterance SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD from_source ON TABLE utterance TYPE record<source>;
DEFINE FIELD raw_text ON TABLE utterance TYPE string;
DEFINE FIELD created_at ON TABLE utterance TYPE datetime VALUE time::now();

-- Raw nodes/edges
DEFINE TABLE nodes SCHEMALESS PERMISSIONS FULL;
DEFINE FIELD type ON TABLE nodes TYPE string;
DEFINE FIELD properties ON TABLE nodes TYPE object;

DEFINE TABLE edges TYPE RELATION FROM nodes TO nodes SCHEMALESS PERMISSIONS FULL;
DEFINE FIELD label ON TABLE edges TYPE string;
DEFINE FIELD properties ON TABLE edges TYPE object;

-- NLU payloads
DEFINE TABLE nlu_data SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD data_bson ON TABLE nlu_data TYPE bytes;
DEFINE FIELD data_json ON TABLE nlu_data TYPE string;
DEFINE FIELD created_at ON TABLE nlu_data TYPE datetime VALUE time::now();

-- Relations in the dynamic graph
DEFINE TABLE has_nlu_output TYPE RELATION FROM utterance TO nlu_data SCHEMAFULL PERMISSIONS FULL;
DEFINE TABLE derived_from TYPE RELATION FROM nodes TO utterance SCHEMALESS PERMISSIONS FULL;
DEFINE TABLE edge_derived_from TYPE RELATION FROM edges TO utterance SCHEMALESS PERMISSIONS FULL;

-- Indexes and search
DEFINE INDEX source_user_id_idx ON TABLE source COLUMNS user_id UNIQUE;
DEFINE INDEX utterance_source_idx ON TABLE utterance COLUMNS from_source;
DEFINE INDEX node_type_idx ON TABLE nodes COLUMNS type;
DEFINE INDEX edge_label_idx ON TABLE edges COLUMNS label;
DEFINE INDEX derived_from_in_idx ON TABLE derived_from COLUMNS in;
DEFINE INDEX derived_from_out_idx ON TABLE derived_from COLUMNS out;
DEFINE INDEX edge_derived_from_in_idx ON TABLE edge_derived_from COLUMNS in;
DEFINE INDEX edge_derived_from_out_idx ON TABLE edge_derived_from COLUMNS out;
DEFINE INDEX node_properties_search_idx ON TABLE nodes COLUMNS properties.* SEARCH ANALYZER ascii;
DEFINE INDEX idx_utterance_nlu_link ON has_nlu_output COLUMNS in, out UNIQUE;
DEFINE INDEX utterance_text_search ON TABLE utterance COLUMNS raw_text SEARCH ANALYZER ascii;

-- Provenance events (PV-01) live in the dynamic DB to allow linking to utterances
DEFINE TABLE provenance_event SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD kind ON TABLE provenance_event TYPE string; -- e.g., scribe_apply, policy_validate, regulariser_fallback
DEFINE FIELD details ON TABLE provenance_event TYPE object;
DEFINE FIELD created_at ON TABLE provenance_event TYPE datetime VALUE time::now();
DEFINE INDEX provenance_event_kind_idx ON TABLE provenance_event COLUMNS kind;

DEFINE TABLE utterance_has_provenance TYPE RELATION FROM utterance TO provenance_event SCHEMAFULL PERMISSIONS FULL;

-- Cross-namespace canonical links are bridged via a local proxy to allow relations in dynamic DB
DEFINE TABLE canonical_ref SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD kind ON TABLE canonical_ref TYPE string; -- entity|event|task
DEFINE FIELD canonical_id ON TABLE canonical_ref TYPE string; -- stringified Thing from canonical ns/db
DEFINE FIELD key ON TABLE canonical_ref TYPE option<string>;
DEFINE FIELD name ON TABLE canonical_ref TYPE option<string>;
DEFINE FIELD extra ON TABLE canonical_ref TYPE option<object>;
DEFINE FIELD created_at ON TABLE canonical_ref TYPE datetime VALUE time::now();
DEFINE INDEX canonical_ref_id_idx ON TABLE canonical_ref COLUMNS canonical_id UNIQUE;

DEFINE TABLE canonical_of TYPE RELATION FROM nodes TO canonical_ref SCHEMAFULL PERMISSIONS FULL;
DEFINE TABLE utterance_mentions TYPE RELATION FROM utterance TO canonical_ref SCHEMAFULL PERMISSIONS FULL;

-- Knowledge externalisation (Phase 5)
DEFINE TABLE proposed_edge SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD in_ref ON TABLE proposed_edge TYPE record<nodes>;
DEFINE FIELD out_ref ON TABLE proposed_edge TYPE record<nodes>;
DEFINE FIELD label ON TABLE proposed_edge TYPE string;
DEFINE FIELD score ON TABLE proposed_edge TYPE number;
DEFINE FIELD status ON TABLE proposed_edge TYPE string; -- pending|accepted|rejected
DEFINE FIELD provenance ON TABLE proposed_edge TYPE option<object>;
DEFINE FIELD created_at ON TABLE proposed_edge TYPE datetime VALUE time::now();
DEFINE INDEX proposed_edge_status_score_idx ON TABLE proposed_edge COLUMNS status, score;

DEFINE TABLE knowledge_embedding SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD node_ref ON TABLE knowledge_embedding TYPE record<nodes>;
DEFINE FIELD model ON TABLE knowledge_embedding TYPE string;
DEFINE FIELD version ON TABLE knowledge_embedding TYPE string;
DEFINE FIELD vector ON TABLE knowledge_embedding TYPE array<number>;
DEFINE FIELD updated_at ON TABLE knowledge_embedding TYPE datetime VALUE time::now();
DEFINE INDEX knowledge_embedding_node_idx ON TABLE knowledge_embedding COLUMNS node_ref;
DEFINE INDEX knowledge_embedding_model_idx ON TABLE knowledge_embedding COLUMNS model;
