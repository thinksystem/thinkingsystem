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

-- Users and Keys
DEFINE TABLE user SCHEMAFULL
    PERMISSIONS FULL;
DEFINE FIELD username ON TABLE user TYPE string;
DEFINE FIELD public_key ON TABLE user TYPE string;
DEFINE FIELD private_key ON TABLE user TYPE string CRYPTO;

-- Messages
DEFINE TABLE messages SCHEMAFULL
    PERMISSIONS FULL;
DEFINE FIELD mid ON TABLE messages TYPE string;
DEFINE FIELD content ON TABLE messages TYPE string CRYPTO;
DEFINE FIELD sender ON TABLE messages TYPE string;
DEFINE FIELD destination ON TABLE messages TYPE object;
DEFINE FIELD timestamp ON TABLE messages TYPE datetime VALUE time::now();
DEFINE FIELD msg_type ON TABLE messages TYPE string;
DEFINE FIELD format ON TABLE messages TYPE option<string>;
DEFINE FIELD encoding ON TABLE messages TYPE option<string>;
DEFINE FIELD refs ON TABLE messages TYPE option<array<string>>;
DEFINE FIELD ttl ON TABLE messages TYPE option<int>;
DEFINE FIELD hops ON TABLE messages TYPE option<int>;
DEFINE FIELD sig ON TABLE messages TYPE option<string>;
DEFINE FIELD metadata ON TABLE messages TYPE option<object>;
DEFINE FIELD reply_info ON TABLE messages TYPE option<object>;
DEFINE FIELD media ON TABLE messages TYPE option<object>;
DEFINE FIELD entities ON TABLE messages TYPE option<array<object>>;
DEFINE FIELD reactions ON TABLE messages TYPE option<array<object>>;

-- Graph Infrastructure
DEFINE TABLE nodes SCHEMALESS
    PERMISSIONS FULL;
DEFINE FIELD type ON TABLE nodes TYPE string;
DEFINE FIELD properties ON TABLE nodes TYPE object;

DEFINE TABLE edges TYPE RELATION FROM nodes TO nodes
    SCHEMALESS
    PERMISSIONS FULL;
DEFINE FIELD label ON TABLE edges TYPE string;
DEFINE FIELD properties ON TABLE edges TYPE object;

-- Message-Node Bridge
DEFINE TABLE message_nodes TYPE RELATION FROM messages TO nodes SCHEMAFULL
    PERMISSIONS FULL;
DEFINE FIELD created_at ON TABLE message_nodes TYPE datetime VALUE time::now();

-- Relays
DEFINE TABLE relay SCHEMAFULL
    PERMISSIONS FULL;
DEFINE FIELD address ON TABLE relay TYPE string;
DEFINE FIELD permanence_hours ON TABLE relay TYPE int;
DEFINE FIELD public_key ON TABLE relay TYPE string;
DEFINE FIELD last_seen ON TABLE relay TYPE datetime VALUE time::now();

-- Peers
DEFINE TABLE peers SCHEMAFULL
    PERMISSIONS FULL;
DEFINE FIELD peer_id ON TABLE peers TYPE string;
DEFINE FIELD address ON TABLE peers TYPE string;
DEFINE FIELD public_key ON TABLE peers TYPE string;
DEFINE FIELD last_seen ON TABLE peers TYPE datetime VALUE time::now();
DEFINE FIELD connection_status ON TABLE peers TYPE string;
DEFINE FIELD capabilities ON TABLE peers TYPE array;

-- Platform Connections
DEFINE TABLE platform_connections SCHEMAFULL
    PERMISSIONS FULL;
DEFINE FIELD user_id ON TABLE platform_connections TYPE string;
DEFINE FIELD platform_type ON TABLE platform_connections TYPE string;
DEFINE FIELD platform_user_id ON TABLE platform_connections TYPE string;
DEFINE FIELD credentials ON TABLE platform_connections TYPE string CRYPTO;
DEFINE FIELD status ON TABLE platform_connections TYPE string;
DEFINE FIELD last_sync ON TABLE platform_connections TYPE datetime VALUE time::now();

-- Indexes
DEFINE INDEX messages_mid_idx ON TABLE messages COLUMNS mid UNIQUE;
DEFINE INDEX messages_sender_idx ON TABLE messages COLUMNS sender;
DEFINE INDEX messages_timestamp_idx ON TABLE messages COLUMNS timestamp;
DEFINE INDEX messages_type_idx ON TABLE messages COLUMNS msg_type;
DEFINE INDEX user_username_idx ON TABLE user COLUMNS username UNIQUE;
DEFINE INDEX peers_peer_id_idx ON TABLE peers COLUMNS peer_id UNIQUE;
DEFINE INDEX platform_user_idx ON TABLE platform_connections COLUMNS user_id, platform_type;
DEFINE INDEX node_type_idx ON TABLE nodes COLUMNS type;
DEFINE INDEX edge_label_idx ON TABLE edges COLUMNS label;
DEFINE INDEX node_properties_search_idx ON TABLE nodes COLUMNS properties.* SEARCH ANALYZER ascii;
DEFINE INDEX edge_properties_search_idx ON TABLE edges COLUMNS properties.* SEARCH ANALYZER ascii;
DEFINE INDEX messages_content_search_idx ON TABLE messages COLUMNS content SEARCH ANALYZER ascii;

-- Constraints
DEFINE FIELD msg_type ON TABLE messages TYPE string ASSERT $value IN ['text', 'presence', 'system', 'ack', 'alert', 'notification'];
DEFINE FIELD type ON TABLE nodes TYPE string ASSERT $value IN ['message', 'user', 'conversation', 'channel', 'thread'] OR $value LIKE 'custom:%';
DEFINE FIELD label ON TABLE edges TYPE string ASSERT $value IN ['replies_to', 'mentions', 'reacts_to', 'forwards', 'participates_in', 'belongs_to', 'sends', 'receives'] OR $value LIKE 'custom:%';
DEFINE FIELD connection_status ON TABLE peers TYPE string ASSERT $value IN ['connected', 'disconnected', 'connecting', 'failed'];
DEFINE FIELD platform_type ON TABLE platform_connections TYPE string ASSERT $value IN ['telegram', 'whatsapp', 'signal', 'discord', 'slack', 'email', 'sms'];
