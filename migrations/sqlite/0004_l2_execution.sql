-- L2 executable fixture journal. Existing released migrations remain unchanged.
ALTER TABLE local_turns ADD COLUMN runtime_version INTEGER NOT NULL DEFAULT 0 CHECK(runtime_version>=0);
ALTER TABLE local_turns ADD COLUMN runtime_blob_id TEXT;
DROP TRIGGER local_turns_immutable;
CREATE TRIGGER local_turns_immutable BEFORE UPDATE ON local_turns WHEN
 NEW.id IS NOT OLD.id OR NEW.workspace_id IS NOT OLD.workspace_id OR NEW.created_at_us IS NOT OLD.created_at_us OR
 NEW.run_id IS NOT OLD.run_id OR NEW.attempt_id IS NOT OLD.attempt_id OR NEW.ordinal IS NOT OLD.ordinal OR
 NEW.state IS NOT OLD.state OR NEW.input_command_seq IS NOT OLD.input_command_seq OR NEW.started_at_us IS NOT OLD.started_at_us OR
 NEW.ended_at_us IS NOT OLD.ended_at_us OR NEW.spec_blob_id IS NOT OLD.spec_blob_id
 BEGIN SELECT RAISE(ABORT,'prepared turn identity is immutable'); END;
CREATE TRIGGER local_turns_runtime_reference BEFORE UPDATE OF runtime_blob_id ON local_turns WHEN NEW.runtime_blob_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM local_blobs b WHERE b.workspace_id=NEW.workspace_id AND b.id=NEW.runtime_blob_id) BEGIN SELECT RAISE(ABORT,'runtime blob outside workspace'); END;
CREATE INDEX local_turns_runtime_blob_fk ON local_turns(workspace_id,runtime_blob_id);
CREATE TABLE local_steps (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL,
 run_id TEXT NOT NULL, attempt_id TEXT NOT NULL, turn_id TEXT NOT NULL,
 turn_ordinal INTEGER NOT NULL CHECK(turn_ordinal>=1), step_ordinal INTEGER NOT NULL CHECK(step_ordinal>=1 AND step_ordinal<=50),
 context_blob_id TEXT NOT NULL, tool_catalog_blob_id TEXT NOT NULL, model_profile_blob_id TEXT NOT NULL,
 sealed_at_us INTEGER NOT NULL, state TEXT NOT NULL CHECK(state IN ('sealed','observed')),
 PRIMARY KEY(workspace_id,id), UNIQUE(workspace_id,run_id,turn_ordinal,step_ordinal),
 UNIQUE(workspace_id,id,run_id),
 FOREIGN KEY(workspace_id,turn_id,run_id,attempt_id) REFERENCES local_turns(workspace_id,id,run_id,attempt_id),
 FOREIGN KEY(workspace_id,context_blob_id) REFERENCES local_blobs(workspace_id,id),
 FOREIGN KEY(workspace_id,tool_catalog_blob_id) REFERENCES local_blobs(workspace_id,id),
 FOREIGN KEY(workspace_id,model_profile_blob_id) REFERENCES local_blobs(workspace_id,id)
) STRICT;
CREATE TABLE local_model_calls (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL, step_id TEXT NOT NULL,
 ordinal INTEGER NOT NULL CHECK(ordinal=1), provider_request_id TEXT, request_digest BLOB NOT NULL CHECK(length(request_digest)=32),
 response_blob_id TEXT, state TEXT NOT NULL CHECK(state IN ('intent','response','unknown')), error_code TEXT,
 input_tokens INTEGER CHECK(input_tokens>=0), output_tokens INTEGER CHECK(output_tokens>=0), cached_tokens INTEGER CHECK(cached_tokens>=0),
 started_at_us INTEGER NOT NULL, ended_at_us INTEGER,
 PRIMARY KEY(workspace_id,id), UNIQUE(workspace_id,step_id,ordinal),
 FOREIGN KEY(workspace_id,step_id) REFERENCES local_steps(workspace_id,id),
 FOREIGN KEY(workspace_id,response_blob_id) REFERENCES local_blobs(workspace_id,id),
 CHECK((state='intent' AND response_blob_id IS NULL AND ended_at_us IS NULL) OR (state IN ('response','unknown') AND response_blob_id IS NOT NULL AND ended_at_us IS NOT NULL)),
 CHECK((input_tokens IS NULL)=(output_tokens IS NULL))
) STRICT;
CREATE TABLE local_invocations (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL, run_id TEXT NOT NULL, step_id TEXT NOT NULL,
 parent_invocation_id TEXT, provider_call_id TEXT, request_origin TEXT NOT NULL CHECK(request_origin='model'), tool_binding_key TEXT NOT NULL,
 idempotency_key TEXT NOT NULL, arguments_blob_id TEXT NOT NULL, arguments_digest BLOB NOT NULL CHECK(length(arguments_digest)=32),
 state TEXT NOT NULL CHECK(state IN ('proposed','dispatched','result','unknown')), result_blob_id TEXT, policy_receipt_blob_id TEXT,
 PRIMARY KEY(workspace_id,id), UNIQUE(workspace_id,run_id,idempotency_key), UNIQUE(workspace_id,step_id,provider_call_id),
 FOREIGN KEY(workspace_id,step_id,run_id) REFERENCES local_steps(workspace_id,id,run_id),
 FOREIGN KEY(workspace_id,arguments_blob_id) REFERENCES local_blobs(workspace_id,id),
 FOREIGN KEY(workspace_id,result_blob_id) REFERENCES local_blobs(workspace_id,id),
 FOREIGN KEY(workspace_id,policy_receipt_blob_id) REFERENCES local_blobs(workspace_id,id),
 CHECK(parent_invocation_id IS NULL), CHECK(state<>'result' OR result_blob_id IS NOT NULL), CHECK(state NOT IN ('proposed','dispatched') OR result_blob_id IS NULL)
) STRICT;
CREATE TABLE local_dispatches (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL, invocation_id TEXT NOT NULL,
 attempt_id TEXT NOT NULL, ordinal INTEGER NOT NULL CHECK(ordinal=1), epoch INTEGER NOT NULL CHECK(epoch>=1),
 state TEXT NOT NULL CHECK(state IN ('started','result','unknown')), started_at_us INTEGER NOT NULL, ended_at_us INTEGER,
 error_code TEXT, provider_request_id TEXT,
 PRIMARY KEY(workspace_id,id), UNIQUE(workspace_id,invocation_id,ordinal),
 FOREIGN KEY(workspace_id,invocation_id) REFERENCES local_invocations(workspace_id,id),
 FOREIGN KEY(workspace_id,attempt_id) REFERENCES local_attempts(workspace_id,id)
) STRICT;
CREATE INDEX local_steps_turn_fk ON local_steps(workspace_id,turn_id,run_id,attempt_id);
CREATE INDEX local_steps_context_fk ON local_steps(workspace_id,context_blob_id);
CREATE INDEX local_steps_catalog_fk ON local_steps(workspace_id,tool_catalog_blob_id);
CREATE INDEX local_steps_model_fk ON local_steps(workspace_id,model_profile_blob_id);
CREATE INDEX local_models_response_fk ON local_model_calls(workspace_id,response_blob_id);
CREATE INDEX local_invocations_result_fk ON local_invocations(workspace_id,result_blob_id);
CREATE INDEX local_invocations_policy_fk ON local_invocations(workspace_id,policy_receipt_blob_id);
CREATE INDEX local_dispatches_attempt_fk ON local_dispatches(workspace_id,attempt_id);
CREATE TABLE local_artifacts (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL,
 owner_actor_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('text','json','csv','patch')),
 current_version_id TEXT, status TEXT NOT NULL CHECK(status='draft'),
 PRIMARY KEY(workspace_id,id),
 FOREIGN KEY(workspace_id,owner_actor_id) REFERENCES local_actors(workspace_id,id),
 FOREIGN KEY(workspace_id,current_version_id,id) REFERENCES local_artifact_versions(workspace_id,id,artifact_id) DEFERRABLE INITIALLY DEFERRED
) STRICT;
CREATE TABLE local_artifact_versions (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL, artifact_id TEXT NOT NULL,
 base_version_id TEXT, run_id TEXT, content_blob_id TEXT NOT NULL, checks_manifest_id TEXT NOT NULL,
 version_no INTEGER NOT NULL CHECK(version_no>=1), candidate_step_id TEXT,
 PRIMARY KEY(workspace_id,id), UNIQUE(workspace_id,id,artifact_id), UNIQUE(workspace_id,artifact_id,version_no), UNIQUE(workspace_id,candidate_step_id),
 FOREIGN KEY(workspace_id,artifact_id) REFERENCES local_artifacts(workspace_id,id),
 FOREIGN KEY(workspace_id,base_version_id,artifact_id) REFERENCES local_artifact_versions(workspace_id,id,artifact_id),
 FOREIGN KEY(workspace_id,run_id) REFERENCES local_runs(workspace_id,id),
 FOREIGN KEY(workspace_id,candidate_step_id,run_id) REFERENCES local_steps(workspace_id,id,run_id),
 FOREIGN KEY(workspace_id,content_blob_id) REFERENCES local_blobs(workspace_id,id),
 FOREIGN KEY(workspace_id,checks_manifest_id) REFERENCES local_manifests(workspace_id,id)
) STRICT;
CREATE INDEX local_artifacts_owner_fk ON local_artifacts(workspace_id,owner_actor_id);
CREATE INDEX local_artifacts_current_fk ON local_artifacts(workspace_id,current_version_id,id);
CREATE INDEX local_artifact_versions_base_fk ON local_artifact_versions(workspace_id,base_version_id,artifact_id);
CREATE INDEX local_artifact_versions_run_fk ON local_artifact_versions(workspace_id,run_id);
CREATE INDEX local_artifact_versions_content_fk ON local_artifact_versions(workspace_id,content_blob_id);
CREATE INDEX local_artifact_versions_checks_fk ON local_artifact_versions(workspace_id,checks_manifest_id);
CREATE TRIGGER local_artifact_versions_immutable BEFORE UPDATE ON local_artifact_versions BEGIN SELECT RAISE(ABORT,'artifact versions are immutable'); END;
CREATE TABLE local_usage (
 id TEXT NOT NULL, workspace_id TEXT NOT NULL, created_at_us INTEGER NOT NULL,
 run_id TEXT NOT NULL, attempt_id TEXT NOT NULL, invocation_id TEXT, meter TEXT NOT NULL, source_key TEXT NOT NULL,
 unit TEXT NOT NULL, quantity_micro INTEGER CHECK(quantity_micro>=0), price_manifest_blob_id TEXT NOT NULL,
 cost_micro INTEGER NOT NULL CHECK(cost_micro=0), quality TEXT NOT NULL CHECK(quality IN ('actual','unknown')), occurred_at_us INTEGER NOT NULL,
 PRIMARY KEY(workspace_id,id), UNIQUE(workspace_id,meter,source_key),
 FOREIGN KEY(workspace_id,run_id) REFERENCES local_runs(workspace_id,id),
 FOREIGN KEY(workspace_id,attempt_id,run_id) REFERENCES local_attempts(workspace_id,id,run_id),
 FOREIGN KEY(workspace_id,invocation_id) REFERENCES local_invocations(workspace_id,id),
 FOREIGN KEY(workspace_id,price_manifest_blob_id) REFERENCES local_blobs(workspace_id,id)
) STRICT;
CREATE INDEX local_usage_attempt_fk ON local_usage(workspace_id,attempt_id,run_id);
CREATE INDEX local_usage_invocation_fk ON local_usage(workspace_id,invocation_id);
CREATE INDEX local_usage_price_fk ON local_usage(workspace_id,price_manifest_blob_id);
CREATE TRIGGER local_usage_immutable BEFORE UPDATE ON local_usage BEGIN SELECT RAISE(ABORT,'usage observations are immutable'); END;
PRAGMA user_version=4;
