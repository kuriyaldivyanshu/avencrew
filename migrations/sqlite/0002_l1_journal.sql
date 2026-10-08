-- L1 local journal slice. Immutable after release; correct with a new migration.
-- Exact column introduction follows the reviewed local catalog; future columns absent.

CREATE TABLE local_tasks (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  owner_actor_id TEXT NOT NULL CHECK (length(owner_actor_id)=36 AND substr(owner_actor_id,9,1)='-' AND substr(owner_actor_id,14,1)='-' AND substr(owner_actor_id,19,1)='-' AND substr(owner_actor_id,24,1)='-' AND length(replace(owner_actor_id,'-',''))=32 AND replace(owner_actor_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  current_revision_id TEXT CHECK (current_revision_id IS NULL OR (length(current_revision_id)=36 AND substr(current_revision_id,9,1)='-' AND substr(current_revision_id,14,1)='-' AND substr(current_revision_id,19,1)='-' AND substr(current_revision_id,24,1)='-' AND length(replace(current_revision_id,'-',''))=32 AND replace(current_revision_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  authority TEXT NOT NULL,
  status TEXT NOT NULL,
  row_version INTEGER NOT NULL CHECK (row_version>=0),
  PRIMARY KEY (workspace_id,id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,owner_actor_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,current_revision_id,id) REFERENCES local_task_revisions(workspace_id,id,task_id) ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  CHECK (authority IN ('local','server')),
  CHECK (status IN ('open','active','waiting','completed','stopped','archived'))
) STRICT;
CREATE INDEX local_tasks_owner_actor_id_fk ON local_tasks(workspace_id,owner_actor_id);
CREATE INDEX local_tasks_current_revision_id_fk ON local_tasks(workspace_id,current_revision_id,id);


CREATE TABLE local_task_revisions (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  task_id TEXT NOT NULL CHECK (length(task_id)=36 AND substr(task_id,9,1)='-' AND substr(task_id,14,1)='-' AND substr(task_id,19,1)='-' AND substr(task_id,24,1)='-' AND length(replace(task_id,'-',''))=32 AND replace(task_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  version_no INTEGER NOT NULL CHECK (version_no>=1),
  schema_version INTEGER NOT NULL CHECK (schema_version>=1),
  canonical_blob_id TEXT NOT NULL CHECK (length(canonical_blob_id)=36 AND substr(canonical_blob_id,9,1)='-' AND substr(canonical_blob_id,14,1)='-' AND substr(canonical_blob_id,19,1)='-' AND substr(canonical_blob_id,24,1)='-' AND length(replace(canonical_blob_id,'-',''))=32 AND replace(canonical_blob_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  digest BLOB NOT NULL CHECK (length(digest)=32),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,task_id,version_no),
  UNIQUE (workspace_id,id,task_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,task_id) REFERENCES local_tasks(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,canonical_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE INDEX local_task_revisions_canonical_blob_id_fk ON local_task_revisions(workspace_id,canonical_blob_id);


CREATE TABLE local_runs (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  task_id TEXT NOT NULL CHECK (length(task_id)=36 AND substr(task_id,9,1)='-' AND substr(task_id,14,1)='-' AND substr(task_id,19,1)='-' AND substr(task_id,24,1)='-' AND length(replace(task_id,'-',''))=32 AND replace(task_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  task_revision_id TEXT NOT NULL CHECK (length(task_revision_id)=36 AND substr(task_revision_id,9,1)='-' AND substr(task_revision_id,14,1)='-' AND substr(task_revision_id,19,1)='-' AND substr(task_revision_id,24,1)='-' AND length(replace(task_revision_id,'-',''))=32 AND replace(task_revision_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  session_id TEXT CHECK (session_id IS NULL OR (length(session_id)=36 AND substr(session_id,9,1)='-' AND substr(session_id,14,1)='-' AND substr(session_id,19,1)='-' AND substr(session_id,24,1)='-' AND length(replace(session_id,'-',''))=32 AND replace(session_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  authority TEXT NOT NULL,
  authority_device_id TEXT CHECK (authority_device_id IS NULL OR (length(authority_device_id)=36 AND substr(authority_device_id,9,1)='-' AND substr(authority_device_id,14,1)='-' AND substr(authority_device_id,19,1)='-' AND substr(authority_device_id,24,1)='-' AND length(replace(authority_device_id,'-',''))=32 AND replace(authority_device_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  status TEXT NOT NULL,
  row_version INTEGER NOT NULL CHECK (row_version>=0),
  fencing_epoch INTEGER NOT NULL CHECK (fencing_epoch>=0),
  accepted_command_seq INTEGER NOT NULL CHECK (accepted_command_seq>=0),
  applied_command_seq INTEGER NOT NULL CHECK (applied_command_seq>=0),
  last_event_seq INTEGER NOT NULL CHECK (last_event_seq>=0),
  current_checkpoint_id TEXT CHECK (current_checkpoint_id IS NULL OR (length(current_checkpoint_id)=36 AND substr(current_checkpoint_id,9,1)='-' AND substr(current_checkpoint_id,14,1)='-' AND substr(current_checkpoint_id,19,1)='-' AND substr(current_checkpoint_id,24,1)='-' AND length(replace(current_checkpoint_id,'-',''))=32 AND replace(current_checkpoint_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  cancellation_requested INTEGER NOT NULL,
  branch_id TEXT CHECK (branch_id IS NULL OR (length(branch_id)=36 AND substr(branch_id,9,1)='-' AND substr(branch_id,14,1)='-' AND substr(branch_id,19,1)='-' AND substr(branch_id,24,1)='-' AND length(replace(branch_id,'-',''))=32 AND replace(branch_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,id,session_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,task_id) REFERENCES local_tasks(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,task_revision_id,task_id) REFERENCES local_task_revisions(workspace_id,id,task_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,session_id) REFERENCES local_sessions(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,authority_device_id) REFERENCES local_devices(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,current_checkpoint_id,id) REFERENCES local_checkpoints(workspace_id,id,run_id) ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (workspace_id,branch_id,session_id) REFERENCES local_session_branches(workspace_id,id,session_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (authority IN ('local','server','sealed','transfer_staging')),
  CHECK (status IN ('queued','preparing','running','waiting','paused','transferring','verifying','recovering','blocked','cancelling','completed','cancelled','failed')),
  CHECK (cancellation_requested IN (0,1)),
  CHECK (applied_command_seq<=accepted_command_seq),
  CHECK ((session_id IS NULL)=(branch_id IS NULL)),
  CHECK (authority<>'local' OR authority_device_id IS NOT NULL),
  CHECK (authority<>'server' OR authority_device_id IS NULL)
) STRICT;
CREATE INDEX local_runs_task_id_fk ON local_runs(workspace_id,task_id);
CREATE INDEX local_runs_task_revision_id_fk ON local_runs(workspace_id,task_revision_id,task_id);
CREATE INDEX local_runs_session_id_fk ON local_runs(workspace_id,session_id);
CREATE INDEX local_runs_authority_device_id_fk ON local_runs(workspace_id,authority_device_id);
CREATE INDEX local_runs_current_checkpoint_id_fk ON local_runs(workspace_id,current_checkpoint_id,id);
CREATE INDEX local_runs_branch_id_fk ON local_runs(workspace_id,branch_id,session_id);


CREATE TABLE local_attempts (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  run_id TEXT NOT NULL CHECK (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  epoch INTEGER NOT NULL CHECK (epoch>=0),
  placement TEXT NOT NULL,
  state TEXT NOT NULL,
  harness_build_digest TEXT NOT NULL CHECK (length(harness_build_digest)=64 AND harness_build_digest NOT GLOB '*[^0-9a-f]*'),
  protocol_version INTEGER NOT NULL CHECK (protocol_version>=1),
  lease_receipt_blob_id TEXT CHECK (lease_receipt_blob_id IS NULL OR (length(lease_receipt_blob_id)=36 AND substr(lease_receipt_blob_id,9,1)='-' AND substr(lease_receipt_blob_id,14,1)='-' AND substr(lease_receipt_blob_id,19,1)='-' AND substr(lease_receipt_blob_id,24,1)='-' AND length(replace(lease_receipt_blob_id,'-',''))=32 AND replace(lease_receipt_blob_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  lease_expires_at_us INTEGER,
  monotonic_deadline_ns INTEGER CHECK (monotonic_deadline_ns>=0),
  boot_id TEXT NOT NULL,
  started_at_us INTEGER NOT NULL,
  ended_at_us INTEGER,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,run_id,epoch),
  UNIQUE (workspace_id,id,run_id,epoch),
  UNIQUE (workspace_id,id,run_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id) REFERENCES local_runs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,lease_receipt_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (placement IN ('local','cloud')),
  CHECK (state IN ('claimed','running','draining','released','lost','fenced')),
  CHECK ((lease_expires_at_us IS NULL)=(monotonic_deadline_ns IS NULL)),
  CHECK (lease_expires_at_us IS NULL OR lease_receipt_blob_id IS NOT NULL)
) STRICT;
CREATE INDEX local_attempts_lease_receipt_blob_id_fk ON local_attempts(workspace_id,lease_receipt_blob_id);
CREATE UNIQUE INDEX local_attempts_active ON local_attempts(workspace_id,run_id) WHERE state IN ('claimed','running','draining');


CREATE TABLE local_commands (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  run_id TEXT NOT NULL CHECK (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  seq INTEGER NOT NULL CHECK (seq>=1),
  actor_id TEXT NOT NULL CHECK (length(actor_id)=36 AND substr(actor_id,9,1)='-' AND substr(actor_id,14,1)='-' AND substr(actor_id,19,1)='-' AND substr(actor_id,24,1)='-' AND length(replace(actor_id,'-',''))=32 AND replace(actor_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  kind TEXT NOT NULL,
  idempotency_key TEXT NOT NULL,
  digest BLOB NOT NULL CHECK (length(digest)=32),
  payload_blob_id TEXT CHECK (payload_blob_id IS NULL OR (length(payload_blob_id)=36 AND substr(payload_blob_id,9,1)='-' AND substr(payload_blob_id,14,1)='-' AND substr(payload_blob_id,19,1)='-' AND substr(payload_blob_id,24,1)='-' AND length(replace(payload_blob_id,'-',''))=32 AND replace(payload_blob_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  expected_row_version INTEGER CHECK (expected_row_version>=0),
  state TEXT NOT NULL,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,run_id,seq),
  UNIQUE (workspace_id,run_id,idempotency_key),
  UNIQUE (workspace_id,id,run_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id) REFERENCES local_runs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,actor_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,payload_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (kind IN ('steer','pause','resume','cancel','change_objective','handoff','human_response')),
  CHECK (state IN ('accepted','applied','rejected'))
) STRICT;
CREATE INDEX local_commands_actor_id_fk ON local_commands(workspace_id,actor_id);
CREATE INDEX local_commands_payload_blob_id_fk ON local_commands(workspace_id,payload_blob_id);


CREATE TABLE local_events (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  run_id TEXT NOT NULL CHECK (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  seq INTEGER NOT NULL CHECK (seq>=1),
  attempt_id TEXT CHECK (attempt_id IS NULL OR (length(attempt_id)=36 AND substr(attempt_id,9,1)='-' AND substr(attempt_id,14,1)='-' AND substr(attempt_id,19,1)='-' AND substr(attempt_id,24,1)='-' AND length(replace(attempt_id,'-',''))=32 AND replace(attempt_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  epoch INTEGER NOT NULL CHECK (epoch>=0),
  event_type TEXT NOT NULL,
  schema_version INTEGER NOT NULL CHECK (schema_version>=1),
  occurred_at_us INTEGER NOT NULL,
  payload_blob_id TEXT CHECK (payload_blob_id IS NULL OR (length(payload_blob_id)=36 AND substr(payload_blob_id,9,1)='-' AND substr(payload_blob_id,14,1)='-' AND substr(payload_blob_id,19,1)='-' AND substr(payload_blob_id,24,1)='-' AND length(replace(payload_blob_id,'-',''))=32 AND replace(payload_blob_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  command_id TEXT CHECK (command_id IS NULL OR (length(command_id)=36 AND substr(command_id,9,1)='-' AND substr(command_id,14,1)='-' AND substr(command_id,19,1)='-' AND substr(command_id,24,1)='-' AND length(replace(command_id,'-',''))=32 AND replace(command_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,run_id,seq),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id) REFERENCES local_runs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,attempt_id,run_id,epoch) REFERENCES local_attempts(workspace_id,id,run_id,epoch) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,payload_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,command_id,run_id) REFERENCES local_commands(workspace_id,id,run_id) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;
CREATE INDEX local_events_attempt_id_fk ON local_events(workspace_id,attempt_id,run_id,epoch);
CREATE INDEX local_events_payload_blob_id_fk ON local_events(workspace_id,payload_blob_id);
CREATE INDEX local_events_command_id_fk ON local_events(workspace_id,command_id,run_id);


CREATE TABLE local_sessions (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  owner_actor_id TEXT NOT NULL CHECK (length(owner_actor_id)=36 AND substr(owner_actor_id,9,1)='-' AND substr(owner_actor_id,14,1)='-' AND substr(owner_actor_id,19,1)='-' AND substr(owner_actor_id,24,1)='-' AND length(replace(owner_actor_id,'-',''))=32 AND replace(owner_actor_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  title TEXT NOT NULL,
  status TEXT NOT NULL,
  PRIMARY KEY (workspace_id,id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,owner_actor_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (status IN ('open','archived','erasing'))
) STRICT;
CREATE INDEX local_sessions_owner_actor_id_fk ON local_sessions(workspace_id,owner_actor_id);


CREATE TABLE local_session_items (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  session_id TEXT NOT NULL CHECK (length(session_id)=36 AND substr(session_id,9,1)='-' AND substr(session_id,14,1)='-' AND substr(session_id,19,1)='-' AND substr(session_id,24,1)='-' AND length(replace(session_id,'-',''))=32 AND replace(session_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  branch_id TEXT NOT NULL CHECK (length(branch_id)=36 AND substr(branch_id,9,1)='-' AND substr(branch_id,14,1)='-' AND substr(branch_id,19,1)='-' AND substr(branch_id,24,1)='-' AND length(replace(branch_id,'-',''))=32 AND replace(branch_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  seq INTEGER NOT NULL CHECK (seq>=1),
  kind TEXT NOT NULL,
  author_id TEXT CHECK (author_id IS NULL OR (length(author_id)=36 AND substr(author_id,9,1)='-' AND substr(author_id,14,1)='-' AND substr(author_id,19,1)='-' AND substr(author_id,24,1)='-' AND length(replace(author_id,'-',''))=32 AND replace(author_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  content_blob_id TEXT CHECK (content_blob_id IS NULL OR (length(content_blob_id)=36 AND substr(content_blob_id,9,1)='-' AND substr(content_blob_id,14,1)='-' AND substr(content_blob_id,19,1)='-' AND substr(content_blob_id,24,1)='-' AND length(replace(content_blob_id,'-',''))=32 AND replace(content_blob_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  run_id TEXT CHECK (run_id IS NULL OR (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  command_id TEXT CHECK (command_id IS NULL OR (length(command_id)=36 AND substr(command_id,9,1)='-' AND substr(command_id,14,1)='-' AND substr(command_id,19,1)='-' AND substr(command_id,24,1)='-' AND length(replace(command_id,'-',''))=32 AND replace(command_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  visibility TEXT NOT NULL,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,branch_id,seq),
  UNIQUE (workspace_id,id,branch_id,session_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,session_id) REFERENCES local_sessions(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,branch_id,session_id) REFERENCES local_session_branches(workspace_id,id,session_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,author_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,content_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id,session_id) REFERENCES local_runs(workspace_id,id,session_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,command_id,run_id) REFERENCES local_commands(workspace_id,id,run_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (kind IN ('message','tool_call','tool_result','control','summary','notice')),
  CHECK (visibility IN ('session','private')),
  CHECK (visibility<>'private' OR author_id IS NOT NULL),
  CHECK (command_id IS NULL OR run_id IS NOT NULL)
) STRICT;
CREATE INDEX local_session_items_session_id_fk ON local_session_items(workspace_id,session_id);
CREATE INDEX local_session_items_branch_id_fk ON local_session_items(workspace_id,branch_id,session_id);
CREATE INDEX local_session_items_author_id_fk ON local_session_items(workspace_id,author_id);
CREATE INDEX local_session_items_content_blob_id_fk ON local_session_items(workspace_id,content_blob_id);
CREATE INDEX local_session_items_run_id_fk ON local_session_items(workspace_id,run_id,session_id);
CREATE INDEX local_session_items_command_id_fk ON local_session_items(workspace_id,command_id,run_id);


CREATE TABLE local_manifests (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  owner_actor_id TEXT NOT NULL CHECK (length(owner_actor_id)=36 AND substr(owner_actor_id,9,1)='-' AND substr(owner_actor_id,14,1)='-' AND substr(owner_actor_id,19,1)='-' AND substr(owner_actor_id,24,1)='-' AND length(replace(owner_actor_id,'-',''))=32 AND replace(owner_actor_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  kind TEXT NOT NULL,
  schema_version INTEGER NOT NULL CHECK (schema_version>=1),
  canonical_blob_id TEXT NOT NULL CHECK (length(canonical_blob_id)=36 AND substr(canonical_blob_id,9,1)='-' AND substr(canonical_blob_id,14,1)='-' AND substr(canonical_blob_id,19,1)='-' AND substr(canonical_blob_id,24,1)='-' AND length(replace(canonical_blob_id,'-',''))=32 AND replace(canonical_blob_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  digest BLOB NOT NULL CHECK (length(digest)=32),
  sealed_at_us INTEGER NOT NULL,
  deletion_generation INTEGER NOT NULL CHECK (deletion_generation>=0),
  PRIMARY KEY (workspace_id,id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,owner_actor_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,canonical_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (kind IN ('workspace','checkpoint','handoff','snapshot','export'))
) STRICT;
CREATE INDEX local_manifests_owner_actor_id_fk ON local_manifests(workspace_id,owner_actor_id);
CREATE INDEX local_manifests_canonical_blob_id_fk ON local_manifests(workspace_id,canonical_blob_id);


CREATE TABLE local_checkpoints (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  run_id TEXT NOT NULL CHECK (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  attempt_id TEXT NOT NULL CHECK (length(attempt_id)=36 AND substr(attempt_id,9,1)='-' AND substr(attempt_id,14,1)='-' AND substr(attempt_id,19,1)='-' AND substr(attempt_id,24,1)='-' AND length(replace(attempt_id,'-',''))=32 AND replace(attempt_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  manifest_id TEXT NOT NULL CHECK (length(manifest_id)=36 AND substr(manifest_id,9,1)='-' AND substr(manifest_id,14,1)='-' AND substr(manifest_id,19,1)='-' AND substr(manifest_id,24,1)='-' AND length(replace(manifest_id,'-',''))=32 AND replace(manifest_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  event_seq INTEGER NOT NULL CHECK (event_seq>=0),
  accepted_command_seq INTEGER NOT NULL CHECK (accepted_command_seq>=0),
  applied_command_seq INTEGER NOT NULL CHECK (applied_command_seq>=0),
  kind TEXT NOT NULL,
  schema_version INTEGER NOT NULL CHECK (schema_version>=1),
  digest BLOB NOT NULL CHECK (length(digest)=32),
  sealed_at_us INTEGER NOT NULL,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,id,run_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id) REFERENCES local_runs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,attempt_id,run_id) REFERENCES local_attempts(workspace_id,id,run_id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,manifest_id) REFERENCES local_manifests(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (kind IN ('logical','filesystem','handoff')),
  CHECK (applied_command_seq<=accepted_command_seq)
) STRICT;
CREATE INDEX local_checkpoints_run_id_fk ON local_checkpoints(workspace_id,run_id);
CREATE INDEX local_checkpoints_attempt_id_fk ON local_checkpoints(workspace_id,attempt_id,run_id);
CREATE INDEX local_checkpoints_manifest_id_fk ON local_checkpoints(workspace_id,manifest_id);


CREATE TABLE local_budget_periods (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  scope_key TEXT NOT NULL,
  unit TEXT NOT NULL,
  currency TEXT,
  starts_at_us INTEGER NOT NULL,
  ends_at_us INTEGER NOT NULL,
  cap_micro INTEGER NOT NULL CHECK (cap_micro>=0),
  held_micro INTEGER NOT NULL CHECK (held_micro>=0),
  settled_micro INTEGER NOT NULL CHECK (settled_micro>=0),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,scope_key,unit,starts_at_us),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (unit IN ('money','input_tokens','output_tokens','compute_seconds','tool_calls','storage_byte_hours')),
  CHECK (ends_at_us>starts_at_us),
  CHECK ((unit='money' AND currency IS NOT NULL AND length(currency)=3 AND currency NOT GLOB '*[^A-Z]*') OR (unit<>'money' AND currency IS NULL))
) STRICT;


CREATE TABLE local_work_queue (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  run_id TEXT CHECK (run_id IS NULL OR (length(run_id)=36 AND substr(run_id,9,1)='-' AND substr(run_id,14,1)='-' AND substr(run_id,19,1)='-' AND substr(run_id,24,1)='-' AND length(replace(run_id,'-',''))=32 AND replace(run_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  kind TEXT NOT NULL,
  dedupe_key TEXT NOT NULL,
  due_at_us INTEGER NOT NULL,
  state TEXT NOT NULL,
  claim_token TEXT,
  lease_until_us INTEGER CHECK (lease_until_us>=0),
  attempt_count INTEGER NOT NULL CHECK (attempt_count>=0),
  spec_blob_id TEXT CHECK (spec_blob_id IS NULL OR (length(spec_blob_id)=36 AND substr(spec_blob_id,9,1)='-' AND substr(spec_blob_id,14,1)='-' AND substr(spec_blob_id,19,1)='-' AND substr(spec_blob_id,24,1)='-' AND length(replace(spec_blob_id,'-',''))=32 AND replace(spec_blob_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  error_code TEXT,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,kind,dedupe_key),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,run_id) REFERENCES local_runs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,spec_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (kind IN ('dispatch','wake','cleanup','reconcile','sync','purge')),
  CHECK (state IN ('ready','claimed','done','dead','cancelled')),
  CHECK ((state='claimed' AND claim_token IS NOT NULL AND lease_until_us IS NOT NULL) OR (state<>'claimed' AND claim_token IS NULL AND lease_until_us IS NULL))
) STRICT;
CREATE INDEX local_work_queue_run_id_fk ON local_work_queue(workspace_id,run_id);
CREATE INDEX local_work_queue_spec_blob_id_fk ON local_work_queue(workspace_id,spec_blob_id);
CREATE INDEX local_work_queue_due ON local_work_queue(state,due_at_us);
CREATE INDEX local_work_queue_expiry ON local_work_queue(state,lease_until_us);


CREATE TABLE local_session_branches (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  session_id TEXT NOT NULL CHECK (length(session_id)=36 AND substr(session_id,9,1)='-' AND substr(session_id,14,1)='-' AND substr(session_id,19,1)='-' AND substr(session_id,24,1)='-' AND length(replace(session_id,'-',''))=32 AND replace(session_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  parent_branch_id TEXT CHECK (parent_branch_id IS NULL OR (length(parent_branch_id)=36 AND substr(parent_branch_id,9,1)='-' AND substr(parent_branch_id,14,1)='-' AND substr(parent_branch_id,19,1)='-' AND substr(parent_branch_id,24,1)='-' AND length(replace(parent_branch_id,'-',''))=32 AND replace(parent_branch_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  fork_item_id TEXT CHECK (fork_item_id IS NULL OR (length(fork_item_id)=36 AND substr(fork_item_id,9,1)='-' AND substr(fork_item_id,14,1)='-' AND substr(fork_item_id,19,1)='-' AND substr(fork_item_id,24,1)='-' AND length(replace(fork_item_id,'-',''))=32 AND replace(fork_item_id,'-','') NOT GLOB '*[^0-9a-f]*')),
  last_seq INTEGER NOT NULL CHECK (last_seq>=0),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,id,session_id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,session_id) REFERENCES local_sessions(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,parent_branch_id,session_id) REFERENCES local_session_branches(workspace_id,id,session_id) ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  FOREIGN KEY (workspace_id,fork_item_id,parent_branch_id,session_id) REFERENCES local_session_items(workspace_id,id,branch_id,session_id) ON UPDATE RESTRICT ON DELETE RESTRICT DEFERRABLE INITIALLY DEFERRED,
  CHECK (parent_branch_id IS NULL OR parent_branch_id<>id),
  CHECK (fork_item_id IS NULL OR parent_branch_id IS NOT NULL)
) STRICT;
CREATE INDEX local_session_branches_session_id_fk ON local_session_branches(workspace_id,session_id);
CREATE INDEX local_session_branches_parent_branch_id_fk ON local_session_branches(workspace_id,parent_branch_id,session_id);
CREATE INDEX local_session_branches_fork_item_id_fk ON local_session_branches(workspace_id,fork_item_id,parent_branch_id,session_id);


CREATE TRIGGER local_runs_state_guard BEFORE UPDATE OF status,cancellation_requested ON local_runs BEGIN
 SELECT RAISE(ABORT,'invalid run transition') WHERE NEW.status<>OLD.status AND NOT (
 (OLD.status='queued' AND NEW.status IN ('preparing','paused','blocked','cancelling','failed')) OR
 (OLD.status='preparing' AND NEW.status IN ('running','recovering','blocked','cancelling','failed')) OR
 (OLD.status='running' AND NEW.status IN ('waiting','paused','transferring','verifying','recovering','blocked','cancelling','failed')) OR
 (OLD.status='waiting' AND NEW.status IN ('running','queued','paused','blocked','cancelling','failed')) OR
 (OLD.status='paused' AND NEW.status IN ('queued','cancelling')) OR
 (OLD.status='blocked' AND NEW.status IN ('queued','cancelling','failed')) OR
 (OLD.status='transferring' AND NEW.status IN ('preparing','paused','blocked','cancelling','failed')) OR
 (OLD.status='verifying' AND NEW.status IN ('completed','running','waiting','recovering','blocked','cancelling','failed')) OR
 (OLD.status='recovering' AND NEW.status IN ('queued','blocked','cancelling','failed')) OR
 (OLD.status='cancelling' AND NEW.status IN ('cancelled','blocked')));
 SELECT RAISE(ABORT,'cancellation is sticky') WHERE OLD.cancellation_requested=1 AND NEW.cancellation_requested<>1;
 SELECT RAISE(ABORT,'cancelled work cannot resume') WHERE NEW.cancellation_requested=1 AND NEW.status NOT IN ('cancelling','blocked','cancelled','failed');
END;

CREATE TRIGGER local_branches_ancestry_insert BEFORE INSERT ON local_session_branches
WHEN NEW.parent_branch_id IS NOT NULL BEGIN
 SELECT RAISE(ABORT,'cyclic or over-limit branch ancestry') WHERE EXISTS (
   WITH RECURSIVE parents(id,parent_branch_id,depth) AS (
     SELECT id,parent_branch_id,1 FROM local_session_branches WHERE workspace_id=NEW.workspace_id AND id=NEW.parent_branch_id
     UNION ALL SELECT b.id,b.parent_branch_id,p.depth+1 FROM local_session_branches b JOIN parents p ON b.id=p.parent_branch_id AND b.workspace_id=NEW.workspace_id WHERE p.depth<128 AND p.id<>NEW.id
   ) SELECT 1 FROM parents WHERE id=NEW.id OR parent_branch_id=NEW.id OR depth=128
 );
END;

CREATE TRIGGER local_branches_ancestry_update BEFORE UPDATE OF parent_branch_id,session_id ON local_session_branches
WHEN NEW.parent_branch_id IS NOT NULL BEGIN
 SELECT RAISE(ABORT,'cyclic or over-limit branch ancestry') WHERE EXISTS (
   WITH RECURSIVE parents(id,parent_branch_id,depth) AS (
     SELECT id,parent_branch_id,1 FROM local_session_branches WHERE workspace_id=NEW.workspace_id AND id=NEW.parent_branch_id
     UNION ALL SELECT b.id,b.parent_branch_id,p.depth+1 FROM local_session_branches b JOIN parents p ON b.id=p.parent_branch_id AND b.workspace_id=NEW.workspace_id WHERE p.depth<128 AND p.id<>NEW.id
   ) SELECT 1 FROM parents WHERE id=NEW.id OR parent_branch_id=NEW.id OR depth=128
 );
END;

-- The single writer serializes overlap checks; honest cost overruns are still
-- recordable (there is deliberately no held+settled<=cap constraint).
CREATE TRIGGER local_budget_periods_overlap_insert BEFORE INSERT ON local_budget_periods BEGIN
 SELECT RAISE(ABORT,'overlapping budget period') WHERE EXISTS (
   SELECT 1 FROM local_budget_periods WHERE workspace_id=NEW.workspace_id AND scope_key=NEW.scope_key AND unit=NEW.unit AND starts_at_us<NEW.ends_at_us AND ends_at_us>NEW.starts_at_us
 );
END;
CREATE TRIGGER local_budget_periods_overlap_update BEFORE UPDATE ON local_budget_periods BEGIN
 SELECT RAISE(ABORT,'overlapping budget period') WHERE EXISTS (
   SELECT 1 FROM local_budget_periods WHERE workspace_id=NEW.workspace_id AND id<>NEW.id AND scope_key=NEW.scope_key AND unit=NEW.unit AND starts_at_us<NEW.ends_at_us AND ends_at_us>NEW.starts_at_us
 );
END;

CREATE TRIGGER local_tasks_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_tasks WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_task_revisions_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_task_revisions WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_runs_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_runs WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_attempts_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_attempts WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_commands_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_commands WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_events_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_events WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_sessions_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_sessions WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_session_items_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_session_items WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_manifests_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_manifests WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_checkpoints_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_checkpoints WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_budget_periods_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_budget_periods WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_work_queue_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_work_queue WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_session_branches_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_session_branches WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_task_revisions_record_immutable BEFORE UPDATE OF id,workspace_id,created_at_us,task_id,version_no,schema_version,canonical_blob_id,digest ON local_task_revisions BEGIN
 SELECT RAISE(ABORT,'committed journal record is immutable');
END;

CREATE TRIGGER local_events_record_immutable BEFORE UPDATE OF id,workspace_id,created_at_us,run_id,seq,attempt_id,epoch,event_type,schema_version,occurred_at_us,payload_blob_id,command_id ON local_events BEGIN
 SELECT RAISE(ABORT,'committed journal record is immutable');
END;

CREATE TRIGGER local_session_items_record_immutable BEFORE UPDATE OF id,workspace_id,created_at_us,session_id,branch_id,seq,kind,author_id,content_blob_id,run_id,command_id,visibility ON local_session_items BEGIN
 SELECT RAISE(ABORT,'committed journal record is immutable');
END;

CREATE TRIGGER local_manifests_record_immutable BEFORE UPDATE OF id,workspace_id,created_at_us,owner_actor_id,kind,schema_version,canonical_blob_id,digest,sealed_at_us,deletion_generation ON local_manifests BEGIN
 SELECT RAISE(ABORT,'committed journal record is immutable');
END;

CREATE TRIGGER local_checkpoints_record_immutable BEFORE UPDATE OF id,workspace_id,created_at_us,run_id,attempt_id,manifest_id,event_seq,accepted_command_seq,applied_command_seq,kind,schema_version,digest,sealed_at_us ON local_checkpoints BEGIN
 SELECT RAISE(ABORT,'committed journal record is immutable');
END;

PRAGMA user_version=2;
