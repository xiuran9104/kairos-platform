use kairos_platform_framework_database::{DatabaseConfig, connect, ensure_initialized};
use sqlx::Row;

#[tokio::test]
#[ignore = "run with script/test-database-migrations.sh"]
async fn applies_all_migrations_to_empty_postgres() {
    let url = std::env::var("TEST_DATABASE_URL").expect("TEST_DATABASE_URL is required");
    let config = DatabaseConfig::new(url, 1, 5, std::time::Duration::from_secs(10))
        .expect("valid test database config");
    let pool = connect(&config).await.expect("connect test database");
    ensure_initialized(&pool)
        .await
        .expect("database bootstrap is initialized");

    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(&pool)
        .await
        .expect("read migration history");
    assert_eq!(applied, 1);

    let storyboard_asset_order_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
           SELECT 1 FROM information_schema.columns
           WHERE table_schema='toonflow'
             AND table_name='assets_storyboards'
             AND column_name='sort_order'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect storyboard asset ordering column");
    assert!(storyboard_asset_order_exists);

    // AI-02: workflow migration columns
    for (table, column) in [
        ("toonflow.projects", "global_seed"),
        ("toonflow.storyboards", "confirm_state"),
        ("toonflow.tasks", "payload"),
        ("toonflow.tasks", "cancel_requested"),
        ("toonflow.tasks", "progress"),
        ("toonflow.assets", "scope"),
        ("toonflow.images", "seed"),
        ("toonflow.videos", "seed"),
    ] {
        let row: Option<(bool,)> = sqlx::query_as(
            "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema||'.'||table_name=$1 AND column_name=$2)",
        )
        .bind(table)
        .bind(column)
        .fetch_optional(&pool)
        .await
        .expect("inspect workflow column");
        let exists = row.is_some_and(|(found,)| found);
        assert!(exists, "workflow column {table}.{column} must exist");
    }

    for table in [
        "ai.model_configs",
        "ai.chat_roles",
        "ai.knowledge_segments",
        "ai.images",
        "ai.music",
        "toonflow.projects",
        "toonflow.project_assets",
        "system_users",
        "system_role",
        "system_menu",
        "system_oauth2_access_token",
        "infra_config",
        "infra_job",
        "infra_job_log",
        "infra_api_access_log",
        "infra_api_error_log",
        "infra_codegen_table",
        "infra_codegen_column",
        "pent_runtime_policies",
        "pent_runs",
        "pent_http_traffic",
        "pent_skills",
        "pent_tool_calls",
        "pent_workers",
        "yudao_demo01_contact",
        "yudao_demo02_category",
        "yudao_demo03_student",
        "yudao_demo03_course",
        "yudao_demo03_grade",
    ] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("inspect expected table");
        assert!(exists, "expected table {table}");
    }
    for removed in ["toonflow.vendor_configs", "toonflow.model_prompts"] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(removed)
            .fetch_one(&pool)
            .await
            .expect("inspect removed table");
        assert!(!exists, "legacy table {removed} must be removed");
    }
    let menu = sqlx::query("SELECT component,deleted FROM system_menu WHERE id=30006")
        .fetch_one(&pool)
        .await
        .expect("AI model menu exists");
    assert_eq!(menu.get::<String, _>("component"), "ai/model/model/index");
    assert_eq!(menu.get::<i16, _>("deleted"), 1);

    let duplicate_route_names: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM (
             SELECT CASE
                    WHEN coalesce(component_name, '') <> '' THEN component_name
                    ELSE name
                    END AS route_name
             FROM system_menu
             WHERE deleted = 0 AND status = 0 AND type <> 3
             GROUP BY route_name
             HAVING count(*) > 1
         ) duplicate_routes",
    )
    .fetch_one(&pool)
    .await
    .expect("read duplicate route names");
    assert_eq!(
        duplicate_route_names, 0,
        "active route menus must not generate duplicate frontend route names"
    );

    let active_menu_links: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_menu
         WHERE deleted = 0 AND visible = false AND active_menu_id IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("read hidden-page business menu links");
    assert!(active_menu_links >= 14);

    let administrators: i64 = sqlx::query_scalar(
        "SELECT count(*)
         FROM system_users u
         JOIN system_user_role ur ON ur.user_id = u.id AND ur.deleted = 0
         JOIN system_role r ON r.id = ur.role_id AND r.deleted = 0
         WHERE u.deleted = 0 AND u.status = 0 AND r.status = 0
           AND r.code = 'super_admin'",
    )
    .fetch_one(&pool)
    .await
    .expect("read seeded administrators");
    assert!(administrators > 0);

    let pent_roles: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_role
         WHERE deleted=0
           AND code IN ('pent_operator','pent_approver','pent_auditor')",
    )
    .fetch_one(&pool)
    .await
    .expect("read seeded Pent roles");
    assert_eq!(
        pent_roles, 9,
        "all Pent operational roles must be seeded for every baseline tenant"
    );

    let pent_action_permissions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_menu
         WHERE deleted=0 AND type=3
           AND permission IN (
               'pent:project:create','pent:task:create','pent:task:cancel',
               'pent:run:execute','pent:approval:decide','pent:evidence:read',
               'pent:artifact:download','pent:tool:manage',
               'pent:rmos:feedback','pent:settings:manage'
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("read Pent action permissions");
    assert_eq!(
        pent_action_permissions, 10,
        "all Pent action permissions must be seeded"
    );

    let pent_runtime_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='pent_runs'
           AND column_name IN (
               'retry_count','next_attempt_at','failure_code','failure_message',
               'cancel_requested_at','execution_cursor'
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect Pent run state columns");
    assert_eq!(pent_runtime_columns, 6);

    let finding_evidence_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='pent_findings'
           AND column_name IN (
               'description','fingerprint','confidence','false_positive_status',
               'reviewed_by','reviewed_at','review_comment'
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect Finding evidence and review columns");
    assert_eq!(finding_evidence_columns, 7);

    let medium_risk_requires_approval: bool = sqlx::query_scalar(
        "SELECT require_approval_for ? 'medium'
         FROM pent_runtime_policies WHERE tenant_id=0",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect default Pent approval policy");
    assert!(medium_risk_requires_approval);

    let active_validators: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pent_tool_definitions
         WHERE lifecycle_status='active' AND tool_id IN (
             'sqli_validate','xss_validate','csrf_validate',
             'command_injection_validate','file_inclusion_validate',
             'file_upload_validate'
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect active vulnerability validators");
    assert_eq!(active_validators, 6);

    // YAML tool recipes: executable tools carry command + args templates in
    // the executor JSON after the recipe import.
    let recipe_backed_tools: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pent_tool_definitions
         WHERE lifecycle_status='active'
           AND executor ? 'args' AND executor ? 'command'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect recipe-backed tool executors");
    assert!(recipe_backed_tools >= 60);

    let pent_event_trigger_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pg_trigger
             WHERE tgname='trg_notify_pent_run_event' AND NOT tgisinternal
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect Pent event notification trigger");
    assert!(pent_event_trigger_exists);

    let baseline_tenants: i64 =
        sqlx::query_scalar("SELECT count(*) FROM system_tenant WHERE deleted = 0")
            .fetch_one(&pool)
            .await
            .expect("read baseline tenants");
    assert_eq!(
        baseline_tenants, 3,
        "fresh migration bootstrap must restore the current baseline tenants, not synthesize a default company"
    );

    let current_baseline_tenant_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM system_tenant
            WHERE id = 1 AND name = '芋道源码' AND deleted = 0
         )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect current baseline tenant");
    assert!(
        current_baseline_tenant_exists,
        "fresh migration bootstrap must use the current database baseline data"
    );

    let legacy_schema_exists: bool =
        sqlx::query_scalar("SELECT to_regnamespace('system') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("inspect legacy schema");
    assert!(!legacy_schema_exists);

    for runtime_table in [
        "system_oauth2_access_token",
        "system_oauth2_refresh_token",
        "system_login_log",
        "system_operate_log",
        "system_notify_message",
    ] {
        let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {runtime_table}"))
            .fetch_one(&pool)
            .await
            .expect("read runtime table");
        assert_eq!(rows, 0, "{runtime_table} must start empty");
    }

    let codex_test_users: i64 =
        sqlx::query_scalar("SELECT count(*) FROM system_users WHERE username='codex_excel_user'")
            .fetch_one(&pool)
            .await
            .expect("read test users");
    assert_eq!(
        codex_test_users, 0,
        "transient test users must not be seeded"
    );

    let users_with_login_traces: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_users WHERE login_ip <> '' OR login_date IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("read user login traces");
    assert_eq!(
        users_with_login_traces, 0,
        "seed users must not carry login traces"
    );

    let users_with_remote_yudao_avatar: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_users
         WHERE avatar LIKE 'http://test.yudao.iocoder.cn/%'
            OR avatar LIKE 'https://test.yudao.iocoder.cn/%'",
    )
    .fetch_one(&pool)
    .await
    .expect("read user avatars");
    assert_eq!(
        users_with_remote_yudao_avatar, 0,
        "seed users must not depend on remote Yudao avatar assets"
    );

    let plain_mail_passwords: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_mail_account
         WHERE password IS NOT NULL AND password <> '' AND password NOT LIKE 'enc:v1:%'",
    )
    .fetch_one(&pool)
    .await
    .expect("read mail secrets");
    assert_eq!(
        plain_mail_passwords, 0,
        "mail account passwords must be sealed"
    );

    let plain_sms_secrets: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_sms_channel
         WHERE (api_key IS NOT NULL AND api_key <> '' AND api_key NOT LIKE 'enc:v1:%')
            OR (api_secret IS NOT NULL AND api_secret <> '' AND api_secret NOT LIKE 'enc:v1:%')",
    )
    .fetch_one(&pool)
    .await
    .expect("read sms secrets");
    assert_eq!(plain_sms_secrets, 0, "sms channel secrets must be sealed");
}
