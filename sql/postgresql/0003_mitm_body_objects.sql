BEGIN;

ALTER TABLE pent_http_traffic
    ADD COLUMN IF NOT EXISTS request_sha256 varchar(64),
    ADD COLUMN IF NOT EXISTS request_body_object_key text,
    ADD COLUMN IF NOT EXISTS response_body_object_key text;

COMMIT;
