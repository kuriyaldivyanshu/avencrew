-- L0 local journal slice. Immutable after release; correct with a new migration.
-- Exact column introduction follows the reviewed local catalog; future columns absent.

CREATE TABLE local_workspaces (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  name TEXT NOT NULL,
  origin_public_key TEXT NOT NULL,
  status TEXT NOT NULL,
  permission_generation INTEGER NOT NULL CHECK (permission_generation>=0),
  deletion_generation INTEGER NOT NULL CHECK (deletion_generation>=0),
  last_local_seq INTEGER NOT NULL CHECK (last_local_seq>=0),
  PRIMARY KEY (id),
  CHECK (workspace_id=id),
  CHECK (status IN ('active','archived','erasing','erased'))
) STRICT;


CREATE TABLE local_actors (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  kind TEXT NOT NULL,
  display_name TEXT NOT NULL,
  status TEXT NOT NULL,
  authority_expires_at_us INTEGER,
  PRIMARY KEY (workspace_id,id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (kind IN ('human','worker','service')),
  CHECK (status IN ('active','disabled','erased'))
) STRICT;


CREATE TABLE local_devices (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  actor_id TEXT NOT NULL CHECK (length(actor_id)=36 AND substr(actor_id,9,1)='-' AND substr(actor_id,14,1)='-' AND substr(actor_id,19,1)='-' AND substr(actor_id,24,1)='-' AND length(replace(actor_id,'-',''))=32 AND replace(actor_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  public_key TEXT NOT NULL,
  keychain_locator TEXT NOT NULL,
  status TEXT NOT NULL,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,public_key),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,actor_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (status IN ('active','revoked','retired'))
) STRICT;
CREATE INDEX local_devices_actor_id_fk ON local_devices(workspace_id,actor_id);


CREATE TABLE local_blobs (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  owner_actor_id TEXT NOT NULL CHECK (length(owner_actor_id)=36 AND substr(owner_actor_id,9,1)='-' AND substr(owner_actor_id,14,1)='-' AND substr(owner_actor_id,19,1)='-' AND substr(owner_actor_id,24,1)='-' AND length(replace(owner_actor_id,'-',''))=32 AND replace(owner_actor_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  storage_relpath TEXT NOT NULL,
  sha256 BLOB NOT NULL CHECK (length(sha256)=32),
  byte_size INTEGER NOT NULL CHECK (byte_size>=0),
  media_type TEXT NOT NULL,
  status TEXT NOT NULL,
  erased_at_us INTEGER,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,storage_relpath),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,owner_actor_id) REFERENCES local_actors(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (status IN ('staging','verified','quarantined','erased')),
  CHECK (storage_relpath<>'' AND substr(storage_relpath,1,1)<>'/' AND instr(storage_relpath,char(0))=0 AND instr(storage_relpath,'\')=0 AND instr('/'||storage_relpath||'/','/../')=0 AND instr('/'||storage_relpath||'/','/./')=0 AND instr(storage_relpath,'//')=0)
) STRICT;
CREATE INDEX local_blobs_owner_actor_id_fk ON local_blobs(workspace_id,owner_actor_id);


CREATE TABLE local_erasure_fences (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  generation INTEGER NOT NULL CHECK (generation>=0),
  selector_blob_id TEXT NOT NULL CHECK (length(selector_blob_id)=36 AND substr(selector_blob_id,9,1)='-' AND substr(selector_blob_id,14,1)='-' AND substr(selector_blob_id,19,1)='-' AND substr(selector_blob_id,24,1)='-' AND length(replace(selector_blob_id,'-',''))=32 AND replace(selector_blob_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  state TEXT NOT NULL,
  completed_at_us INTEGER,
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,generation,id),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  FOREIGN KEY (workspace_id,selector_blob_id) REFERENCES local_blobs(workspace_id,id) ON UPDATE RESTRICT ON DELETE RESTRICT,
  CHECK (state IN ('fenced','purging','completed','blocked')),
  CHECK ((state='completed')=(completed_at_us IS NOT NULL))
) STRICT;
CREATE INDEX local_erasure_fences_selector_blob_id_fk ON local_erasure_fences(workspace_id,selector_blob_id);


CREATE TABLE local_schema_releases (
  id TEXT NOT NULL CHECK (length(id)=36 AND substr(id,9,1)='-' AND substr(id,14,1)='-' AND substr(id,19,1)='-' AND substr(id,24,1)='-' AND length(replace(id,'-',''))=32 AND replace(id,'-','') NOT GLOB '*[^0-9a-f]*'),
  workspace_id TEXT NOT NULL CHECK (length(workspace_id)=36 AND substr(workspace_id,9,1)='-' AND substr(workspace_id,14,1)='-' AND substr(workspace_id,19,1)='-' AND substr(workspace_id,24,1)='-' AND length(replace(workspace_id,'-',''))=32 AND replace(workspace_id,'-','') NOT GLOB '*[^0-9a-f]*'),
  created_at_us INTEGER NOT NULL,
  version INTEGER NOT NULL CHECK (version>=1),
  checksum BLOB NOT NULL CHECK (length(checksum)=32),
  applied_at_us INTEGER NOT NULL,
  minimum_reader_version INTEGER NOT NULL CHECK (minimum_reader_version>=1),
  PRIMARY KEY (workspace_id,id),
  UNIQUE (workspace_id,version),
  FOREIGN KEY (workspace_id) REFERENCES local_workspaces(id) ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT;


CREATE TRIGGER local_workspaces_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_workspaces WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_actors_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_actors WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_devices_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_devices WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_blobs_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_blobs WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_erasure_fences_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_erasure_fences WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_schema_releases_identity_immutable BEFORE UPDATE OF id,workspace_id,created_at_us ON local_schema_releases WHEN NEW.id<>OLD.id OR NEW.workspace_id<>OLD.workspace_id OR NEW.created_at_us<>OLD.created_at_us BEGIN
 SELECT RAISE(ABORT,'journal identity is immutable');
END;

CREATE TRIGGER local_schema_releases_record_immutable BEFORE UPDATE OF id,workspace_id,created_at_us,version,checksum,applied_at_us,minimum_reader_version ON local_schema_releases BEGIN
 SELECT RAISE(ABORT,'committed journal record is immutable');
END;

PRAGMA user_version=1;
