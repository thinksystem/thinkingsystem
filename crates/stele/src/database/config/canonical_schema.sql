-- SPDX-License-Identifier: AGPL-3.0-only
-- Canonical (structured) schema lives in a separate namespace/DB

DEFINE ANALYZER ascii TOKENIZERS class FILTERS lowercase, ascii;

-- Canonical entities, tasks, events, and relationship facts
DEFINE TABLE canonical_entity SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD entity_type ON TABLE canonical_entity TYPE string;
DEFINE FIELD name ON TABLE canonical_entity TYPE string;
DEFINE FIELD canonical_key ON TABLE canonical_entity TYPE string;
DEFINE FIELD extra ON TABLE canonical_entity TYPE object;
DEFINE FIELD created_at ON TABLE canonical_entity TYPE datetime VALUE time::now();
DEFINE FIELD updated_at ON TABLE canonical_entity TYPE option<datetime>;

DEFINE INDEX canonical_entity_key_idx ON TABLE canonical_entity COLUMNS canonical_key UNIQUE;
DEFINE INDEX canonical_entity_name_search ON TABLE canonical_entity COLUMNS name SEARCH ANALYZER ascii;
DEFINE INDEX canonical_entity_type_idx ON TABLE canonical_entity COLUMNS entity_type;

DEFINE TABLE canonical_event SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD title ON TABLE canonical_event TYPE string;
DEFINE FIELD description ON TABLE canonical_event TYPE string;
DEFINE FIELD start_at ON TABLE canonical_event TYPE option<datetime>;
DEFINE FIELD end_at ON TABLE canonical_event TYPE option<datetime>;
DEFINE FIELD timezone ON TABLE canonical_event TYPE option<string>;
DEFINE FIELD recurrence ON TABLE canonical_event TYPE option<string>;
DEFINE FIELD location ON TABLE canonical_event TYPE option<string>;
DEFINE FIELD status ON TABLE canonical_event TYPE option<string>;
DEFINE FIELD confidence ON TABLE canonical_event TYPE option<number>;
DEFINE FIELD provenance ON TABLE canonical_event TYPE option<object>;
DEFINE FIELD created_at ON TABLE canonical_event TYPE datetime VALUE time::now();
DEFINE FIELD updated_at ON TABLE canonical_event TYPE option<datetime>;

DEFINE INDEX canonical_event_time_idx ON TABLE canonical_event COLUMNS start_at, end_at;
DEFINE INDEX canonical_event_status_idx ON TABLE canonical_event COLUMNS status;

DEFINE TABLE canonical_task SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD title ON TABLE canonical_task TYPE string;
DEFINE FIELD due_at ON TABLE canonical_task TYPE option<datetime>;
DEFINE FIELD priority ON TABLE canonical_task TYPE option<string>;
DEFINE FIELD status ON TABLE canonical_task TYPE option<string>;
DEFINE FIELD assignee_ref ON TABLE canonical_task TYPE option<record<canonical_entity>>;
DEFINE FIELD project_ref ON TABLE canonical_task TYPE option<record<canonical_entity>>;
DEFINE FIELD confidence ON TABLE canonical_task TYPE option<number>;
DEFINE FIELD provenance ON TABLE canonical_task TYPE option<object>;
DEFINE FIELD created_at ON TABLE canonical_task TYPE datetime VALUE time::now();
DEFINE FIELD updated_at ON TABLE canonical_task TYPE option<datetime>;

DEFINE INDEX canonical_task_due_idx ON TABLE canonical_task COLUMNS due_at;
DEFINE INDEX canonical_task_status_idx ON TABLE canonical_task COLUMNS status;
DEFINE INDEX canonical_task_assignee_idx ON TABLE canonical_task COLUMNS assignee_ref;

DEFINE TABLE canonical_relationship_fact SCHEMAFULL PERMISSIONS FULL;
-- Allow facts between entities and events (e.g., person ATTENDS event)
DEFINE FIELD subject_ref ON TABLE canonical_relationship_fact TYPE record<canonical_entity> | record<canonical_event>;
DEFINE FIELD predicate ON TABLE canonical_relationship_fact TYPE string;
DEFINE FIELD object_ref ON TABLE canonical_relationship_fact TYPE record<canonical_entity> | record<canonical_event>;
-- Bitemporal + versioning + branching support
-- valid_from/valid_to: business (valid) time; tx_from/tx_to: system (transaction) time
DEFINE FIELD valid_from ON TABLE canonical_relationship_fact TYPE datetime VALUE time::now();
DEFINE FIELD valid_to ON TABLE canonical_relationship_fact TYPE option<datetime>;
DEFINE FIELD tx_from ON TABLE canonical_relationship_fact TYPE datetime VALUE time::now();
DEFINE FIELD tx_to ON TABLE canonical_relationship_fact TYPE option<datetime>;
DEFINE FIELD version_no ON TABLE canonical_relationship_fact TYPE int VALUE 1;
DEFINE FIELD supersedes ON TABLE canonical_relationship_fact TYPE option<record<canonical_relationship_fact>>;
-- Branch / hypothesis tracking
DEFINE FIELD branch_id ON TABLE canonical_relationship_fact TYPE option<string>; -- e.g. default null implies mainline
DEFINE FIELD branch_status ON TABLE canonical_relationship_fact TYPE option<string>; -- hypothesis|active|rejected
DEFINE FIELD promotion_rule ON TABLE canonical_relationship_fact TYPE option<string>; -- serialized rule id / hash used at promotion time
-- Legacy placeholder field retained (effective_at) for backward compatibility; prefer valid_from
DEFINE FIELD effective_at ON TABLE canonical_relationship_fact TYPE option<datetime>;
DEFINE FIELD confidence ON TABLE canonical_relationship_fact TYPE option<number>;
DEFINE FIELD provenance ON TABLE canonical_relationship_fact TYPE option<object>;
DEFINE FIELD created_at ON TABLE canonical_relationship_fact TYPE datetime VALUE time::now();

DEFINE INDEX canonical_fact_predicate_idx ON TABLE canonical_relationship_fact COLUMNS predicate;
DEFINE INDEX canonical_fact_subject_idx ON TABLE canonical_relationship_fact COLUMNS subject_ref;
DEFINE INDEX canonical_fact_object_idx ON TABLE canonical_relationship_fact COLUMNS object_ref;
DEFINE INDEX canonical_fact_time_idx ON TABLE canonical_relationship_fact COLUMNS valid_from, valid_to;
DEFINE INDEX canonical_fact_branch_idx ON TABLE canonical_relationship_fact COLUMNS branch_id;
DEFINE INDEX canonical_fact_version_idx ON TABLE canonical_relationship_fact COLUMNS version_no;

-- Hybrid relationship node (gated logically by feature flag; schema always present is harmless)
DEFINE TABLE relationship_node SCHEMAFULL PERMISSIONS FULL;
DEFINE FIELD subject_ref ON TABLE relationship_node TYPE record<canonical_entity> | record<canonical_event>;
DEFINE FIELD object_ref ON TABLE relationship_node TYPE record<canonical_entity> | record<canonical_event>;
DEFINE FIELD predicate ON TABLE relationship_node TYPE string;
DEFINE FIELD confidence ON TABLE relationship_node TYPE option<number>;
DEFINE FIELD provenance ON TABLE relationship_node TYPE option<object>;
DEFINE FIELD created_at ON TABLE relationship_node TYPE datetime VALUE time::now();
DEFINE INDEX relationship_node_predicate_idx ON TABLE relationship_node COLUMNS predicate;
DEFINE INDEX relationship_node_subject_idx ON TABLE relationship_node COLUMNS subject_ref;
DEFINE INDEX relationship_node_object_idx ON TABLE relationship_node COLUMNS object_ref;
