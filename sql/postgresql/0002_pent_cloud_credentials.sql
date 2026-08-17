BEGIN;

CREATE TABLE IF NOT EXISTS pent_cloud_credentials (
    id varchar(50) PRIMARY KEY,
    tenant_id bigint NOT NULL,
    name varchar(200) NOT NULL,
    provider varchar(30) NOT NULL,
    secret_payload text NOT NULL,
    allowed_tools jsonb NOT NULL DEFAULT '[]'::jsonb,
    status varchar(20) NOT NULL DEFAULT 'active',
    expires_at timestamptz,
    created_by varchar(64) NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT pent_cloud_credentials_provider_check
        CHECK (provider IN ('aws', 'azure', 'gcp', 'aliyun', 'tencent')),
    CONSTRAINT pent_cloud_credentials_status_check
        CHECK (status IN ('active', 'disabled', 'revoked')),
    CONSTRAINT pent_cloud_credentials_allowed_tools_array_check
        CHECK (jsonb_typeof(allowed_tools) = 'array'),
    CONSTRAINT pent_cloud_credentials_encrypted_payload_check
        CHECK (secret_payload LIKE 'env:%')
);

CREATE INDEX IF NOT EXISTS pent_cloud_credentials_tenant_status_idx
    ON pent_cloud_credentials(tenant_id, status);

COMMIT;
