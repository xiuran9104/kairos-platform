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

    for table in [
        "ai.model_configs",
        "ai.chat_roles",
        "ai.knowledge_segments",
        "ai.images",
        "ai.music",
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
        "pent_mcp_servers",
        "pent_knowledge_retrieval_logs",
        "pent_skill_stats_meta",
        "pent_agent_roles",
        "pent_assets",
        "pent_facts",
        "pent_fact_edges",
        "pent_workflows",
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
    let toon_schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM pg_namespace WHERE nspname IN ('toon', 'toonflow')
        )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect removed Toon schemas");
    assert!(!toon_schema_exists, "Toon schemas must be removed");
    let menu = sqlx::query("SELECT component,deleted FROM system_menu WHERE id=30006")
        .fetch_one(&pool)
        .await
        .expect("AI model menu exists");
    assert_eq!(menu.get::<String, _>("component"), "ai/model/model/index");
    assert_eq!(menu.get::<i16, _>("deleted"), 1);

    let pent_capability_ops_menus: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_menu
         WHERE deleted = 0 AND status = 0
           AND id IN (30500, 30501, 30502, 30503, 30504, 30505, 30506, 30507, 30508,
                      30510, 30511, 30512, 30513, 30514, 30515, 30516, 30517,
                      30518, 30519)",
    )
    .fetch_one(&pool)
    .await
    .expect("read pent capability/ops menus");
    assert_eq!(
        pent_capability_ops_menus, 19,
        "pent capability/ops menus must be bootstrapped"
    );

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
               'pent:run:execute','pent:evidence:read',
               'pent:artifact:download','pent:tool:manage',
               'pent:rmos:feedback','pent:settings:manage',
               'pent:role:manage'
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

    let cloud_hierarchy_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public'
           AND (table_name,column_name) IN (
               ('scan_cloud_zones','zone_type'),
               ('scan_cloud_zones','provider_id'),
               ('scan_machine_rooms','platform_id')
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect cloud resource hierarchy columns");
    assert_eq!(cloud_hierarchy_columns, 3);

    let machine_room_has_duplicate_provider: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM information_schema.columns
            WHERE table_schema='public' AND table_name='scan_machine_rooms'
              AND column_name='provider_id'
        )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect machine room ownership columns");
    assert!(
        !machine_room_has_duplicate_provider,
        "machine room provider must be derived through its cloud platform"
    );

    let platform_has_duplicate_provider: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM information_schema.columns
            WHERE table_schema='public' AND table_name='scan_cloud_platforms'
              AND column_name='provider_id'
        )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect cloud platform ownership columns");
    assert!(
        !platform_has_duplicate_provider,
        "cloud platform provider must be derived through its cloud zone"
    );

    let zone_provider_binding_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public.scan_cloud_zone_providers') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("inspect cloud zone provider binding table");
    assert!(
        !zone_provider_binding_exists,
        "a cloud zone belongs to one operator directly; no join table is needed"
    );

    let provider_config_binds_only_platform: bool = sqlx::query_scalar(
        "SELECT
           EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='scan_cloud_provider_configs' AND column_name='platform_id')
           AND NOT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema='public' AND table_name='scan_cloud_provider_configs' AND column_name='zone_id')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect cloud provider config hierarchy columns");
    assert!(
        provider_config_binds_only_platform,
        "cloud provider config must bind only to its cloud platform"
    );

    let provider_auth_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_cloud_provider_configs'
           AND column_name IN ('auth_type','auth_username','auth_password','auth_domain','project_id')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect cloud provider authentication columns");
    assert_eq!(provider_auth_columns, 5);

    let cloud_instance_source_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_business_resources'
           AND column_name='cloud_provider_config_id' AND is_nullable='YES')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect cloud instance reconciliation source");
    assert!(cloud_instance_source_exists);

    let physical_resource_location_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_business_resources'
           AND column_name='machine_room_id' AND is_nullable='YES')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect optional physical resource machine room");
    assert!(physical_resource_location_exists);

    let physical_resource_network_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_business_resources'
           AND column_name IN ('deployment_type','management_ip','business_ip','network_cidr','gateway','vlan_id','dns_servers','mac_address','switch_name','switch_port')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect physical resource network columns");
    assert_eq!(physical_resource_network_columns, 10);

    let physical_rack_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_business_resources'
           AND column_name IN ('cabinet_id','cabinet_name','chassis_name','chassis_slot','rack_start_u','rack_end_u')",
    ).fetch_one(&pool).await.expect("inspect physical rack location columns");
    assert_eq!(physical_rack_columns, 6);

    for table in ["scan_cabinet_rows", "scan_cabinets"] {
        let exists: bool = sqlx::query_scalar("SELECT to_regclass($1) IS NOT NULL")
            .bind(format!("public.{table}"))
            .fetch_one(&pool)
            .await
            .expect("inspect cabinet inventory table");
        assert!(exists, "expected cabinet inventory table {table}");
    }

    let infrastructure_device_table_exists: bool =
        sqlx::query_scalar("SELECT to_regclass('public.scan_infrastructure_devices') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .expect("inspect infrastructure device table");
    assert!(infrastructure_device_table_exists);

    let infrastructure_menu_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM system_menu WHERE id IN (30311,30313,30314,30315,30316,30317)
           AND parent_id=30310 AND deleted=0
           AND name IN ('机房管理','安全设备','物理服务器','网络设备','存储设备','机柜布局')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect infrastructure menus");
    assert_eq!(infrastructure_menu_count, 6);

    let scan_task_scanner_column: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_tasks' AND column_name='scanner_id'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect scan task scanner binding");
    assert_eq!(scan_task_scanner_column, 1);

    let scanner_menu_name: String =
        sqlx::query_scalar("SELECT name FROM system_menu WHERE id=30304 AND deleted=0")
            .fetch_one(&pool)
            .await
            .expect("inspect scanner menu name");
    assert_eq!(scanner_menu_name, "扫描节点");

    let scanner_agent_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_scanners'
           AND column_name IN ('registration_token_hash','token_expires_at','agent_status','agent_version','agent_hostname','agent_ip','capabilities','registered_at','last_heartbeat')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect scanner agent registration columns");
    assert_eq!(scanner_agent_columns, 9);

    let service_provider_menu_name: String =
        sqlx::query_scalar("SELECT name FROM system_menu WHERE id=30312 AND deleted=0")
            .fetch_one(&pool)
            .await
            .expect("inspect service provider menu name");
    assert_eq!(service_provider_menu_name, "服务商");

    let resource_ticket_workflow_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_resource_tickets'
           AND column_name IN ('applicant_name','application_name','contract_name','data_disk','expire_at','security_products','delivery_status')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect resource ticket workflow columns");
    assert_eq!(resource_ticket_workflow_columns, 7);

    let resource_ticket_approval_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='scan_resource_tickets'
           AND column_name IN ('ticket_type','risk_level','target_resource_id','maintenance_window','backup_confirmed','rollback_plan','approval_stage','approval_total','current_approval_role')",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect multi-level approval columns");
    assert_eq!(resource_ticket_approval_columns, 9);
    let approval_history_exists: bool = sqlx::query_scalar(
        "SELECT to_regclass('public.scan_resource_ticket_approvals') IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect resource ticket approval history table");
    assert!(approval_history_exists);

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

    let hitl_policy_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='pent_runtime_policies'
           AND column_name IN (
               'hitl_enabled','hitl_mode','hitl_reviewer','hitl_tool_whitelist',
               'hitl_audit_model_id','hitl_audit_system_prompt','hitl_audit_user_prompt'
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect HITL policy columns");
    assert_eq!(hitl_policy_columns, 7);

    let agent_config_valid: bool = sqlx::query_scalar(
        "SELECT jsonb_typeof(agent_config)='object'
                AND agent_config->>'orchestration'='deep'
                AND (agent_config->>'run_retry_max_attempts')::int=4
         FROM pent_runtime_policies WHERE tenant_id=0",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect DesRed-compatible Agent defaults");
    assert!(agent_config_valid);

    let hitl_approval_columns: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM information_schema.columns
         WHERE table_schema='public' AND table_name='pent_approvals'
           AND column_name IN (
               'hitl_mode','reviewer','original_arguments','effective_arguments',
               'audit_request','audit_response','decision_comment','resumed_at'
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect HITL approval columns");
    assert_eq!(hitl_approval_columns, 8);

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
