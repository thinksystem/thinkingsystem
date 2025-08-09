-- SPDX-License-Identifier: AGPL-3.0-only
-- Copyright (C) 2024 Jonathan Lee
-- This program is free software: you can redistribute it and/or modify
-- it under the terms of the GNU Affero General Public License version 3
-- as published by the Free Software Foundation.
-- This program is distributed in the hope that it will be useful,
-- but WITHOUT ANY WARRANTY; without even the implied warranty of
-- MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
-- See the GNU Affero General Public License for more details.
-- You should have received a copy of the GNU Affero General Public License
-- along with this program. If not, see https://www.gnu.org/licenses/.

DEFINE ANALYZER ascii TOKENIZERS class FILTERS lowercase, ascii;

DEFINE TABLE source SCHEMAFULL
    PERMISSIONS FULL;

DEFINE FIELD user_id ON TABLE source TYPE string;
DEFINE FIELD channel ON TABLE source TYPE string;
DEFINE FIELD properties ON TABLE source TYPE object;
DEFINE FIELD created_at ON TABLE source TYPE datetime VALUE time::now();

DEFINE TABLE utterance SCHEMAFULL
    PERMISSIONS FULL;

DEFINE FIELD from_source ON TABLE utterance TYPE record<source>;
DEFINE FIELD raw_text ON TABLE utterance TYPE string;
DEFINE FIELD created_at ON TABLE utterance TYPE datetime VALUE time::now();

DEFINE TABLE nodes SCHEMALESS
    PERMISSIONS FULL;

DEFINE TABLE nlu_data SCHEMAFULL
    PERMISSIONS FULL;

DEFINE FIELD data_bson ON TABLE nlu_data TYPE bytes;

DEFINE FIELD data_json ON TABLE nlu_data TYPE string;

DEFINE FIELD created_at ON TABLE nlu_data TYPE datetime VALUE time::now();


DEFINE TABLE has_nlu_output TYPE RELATION FROM utterance TO nlu_data
    SCHEMAFULL
    PERMISSIONS FULL;

DEFINE TABLE edges TYPE RELATION FROM nodes TO nodes
    SCHEMALESS
    PERMISSIONS FULL;

DEFINE FIELD type ON TABLE nodes TYPE string;
DEFINE FIELD label ON TABLE edges TYPE string;
DEFINE FIELD properties ON TABLE nodes TYPE object;
DEFINE FIELD properties ON TABLE edges TYPE object;

DEFINE TABLE derived_from TYPE RELATION FROM nodes TO utterance
    SCHEMALESS
    PERMISSIONS FULL;

DEFINE INDEX source_user_id_idx ON TABLE source COLUMNS user_id UNIQUE;
DEFINE INDEX utterance_source_idx ON TABLE utterance COLUMNS from_source;
DEFINE INDEX node_type_idx ON TABLE nodes COLUMNS type;
DEFINE INDEX edge_label_idx ON TABLE edges COLUMNS label;
DEFINE INDEX node_properties_search_idx ON TABLE nodes COLUMNS properties.* SEARCH ANALYZER ascii;
DEFINE INDEX idx_utterance_nlu_link ON has_nlu_output COLUMNS in, out UNIQUE;
DEFINE INDEX utterance_text_search ON TABLE utterance COLUMNS raw_text SEARCH ANALYZER ascii;
