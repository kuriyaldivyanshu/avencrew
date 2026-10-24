-- L2 immutable prepared-turn publication. No model/tool execution is enabled.
CREATE TABLE local_turns (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  run_id TEXT NOT NULL CHECK (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  attempt_id TEXT NOT NULL CHECK (length(attempt_id)=36 AND substr(attempt_id,9,1)='-' AND substr(attempt_id,14,1)='-' AND substr(attempt_id,19,1)='-' AND substr(attempt_id,24,1)='-' AND length(replace(attempt_id,'-',''))=32 AND replace(attempt_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  ordinal INTEGER NOT NULL CHECK (ordinal>=1),
  state TEXT NOT NULL CHECK (state='prepared'),
  input_command_seq INTEGER NOT NULL CHECK (input_command_seq>=0),
  started_at_us INTEGER NOT NULL,
  ended_at_us INTEGER CHECK (ended_at_us IS NULL),
  spec_blob_id TEXT NOT NULL CHECK (length(spec_blob_id)=36 AND substr(spec_blob_id,9,1)='-' AND substr(spec_blob_id,14,1)='-' AND substr(spec_blob_id,19,1)='-' AND substr(spec_blob_id,24,1)='-' AND length(replace(spec_blob_id,'-',''))=32 AND replace(spec_blob_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,run_id,ordinal),
  UNIQUE (workspace_id,id,run_id,attempt_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id) REFERENCES local_runs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,attempt_id,run_id) REFERENCES local_attempts(workspace_id,id,run_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,spec_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE INDEX local_turns_attempt_id_fk ON local_turns(workspace_id,attempt_id,run_id);
CREATE INDEX local_turns_spec_blob_id_fk ON local_turns(workspace_id,spec_blob_id);
CREATE TRIGGER local_turns_immutable BEFORE UPDATE ON local_turns BEGIN SELECT RAISE(ABORT,'prepared turn is immutable'); END;
PRAGMA user_version=3;
