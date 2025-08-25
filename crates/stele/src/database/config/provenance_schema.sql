-- SPDX-License-Identifier: AGPL-3.0-only
-- Provenance DAG schema: execution_event -> prov_edge -> commit_event

-- Execution events (who/where/when an operation ran)
DEFINE TABLE execution_event SCHEMALESS PERMISSIONS FULL;
DEFINE FIELD session_id ON TABLE execution_event TYPE string;
DEFINE FIELD flow_id ON TABLE execution_event TYPE option<string>;
DEFINE FIELD theatre_id ON TABLE execution_event TYPE option<string>;
DEFINE FIELD block_id ON TABLE execution_event TYPE option<string>;
DEFINE FIELD created_at ON TABLE execution_event TYPE datetime VALUE time::now();

-- Commit events (what canonical_event was produced/committed)
DEFINE TABLE commit_event SCHEMALESS PERMISSIONS FULL;
DEFINE FIELD event_id ON TABLE commit_event TYPE record<canonical_event>;
DEFINE FIELD session_id ON TABLE commit_event TYPE string; -- denormalized for fast querying
DEFINE FIELD created_at ON TABLE commit_event TYPE datetime VALUE time::now();
DEFINE FIELD exec_id ON TABLE commit_event TYPE record<execution_event>;
DEFINE INDEX commit_event_event_idx ON TABLE commit_event COLUMNS event_id;
DEFINE INDEX commit_event_session_idx ON TABLE commit_event COLUMNS session_id;
DEFINE INDEX commit_event_exec_idx ON TABLE commit_event COLUMNS exec_id;

-- Edge: execution -> commit (directional)
DEFINE TABLE prov_edge TYPE RELATION IN commit_event OUT execution_event SCHEMALESS PERMISSIONS FULL;
DEFINE FIELD relation ON TABLE prov_edge TYPE string;
