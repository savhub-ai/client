// @generated automatically by Diesel CLI.

diesel::table! {
    ai_request_cache (id) {
        id -> Uuid,
        task_type -> Text,
        target_type -> Text,
        target_id -> Uuid,
        commit_sha -> Text,
        success -> Bool,
        error_message -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    ai_usage_logs (id) {
        id -> Uuid,
        task_type -> Text,
        provider -> Text,
        model -> Text,
        prompt_tokens -> Int4,
        completion_tokens -> Int4,
        total_tokens -> Int4,
        target_type -> Nullable<Text>,
        target_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    audit_logs (id) {
        id -> Uuid,
        actor_user_id -> Nullable<Uuid>,
        action -> Text,
        target_type -> Text,
        target_id -> Nullable<Uuid>,
        metadata -> Jsonb,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    flocks (id) {
        id -> Uuid,
        sign -> Text,
        slug -> Text,
        name -> Text,
        path -> Nullable<Text>,
        repo_id -> Uuid,
        keywords -> Array<Nullable<Text>>,
        description -> Text,
        version -> Nullable<Text>,
        status -> Text,
        visibility -> Nullable<Text>,
        license -> Nullable<Text>,
        metadata -> Jsonb,
        source -> Jsonb,
        imported_by_user_id -> Uuid,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        stats_comments -> Int8,
        stats_ratings -> Int8,
        stats_avg_rating -> Float8,
        security_status -> Text,
        stats_stars -> Int8,
        stats_max_installs -> Int8,
        stats_max_unique_users -> Int8,
    }
}

diesel::table! {
    index_jobs (id) {
        id -> Uuid,
        status -> Text,
        job_type -> Text,
        git_url -> Text,
        git_ref -> Text,
        git_subdir -> Text,
        repo_slug -> Nullable<Text>,
        requested_by_user_id -> Uuid,
        result_data -> Jsonb,
        error_message -> Nullable<Text>,
        started_at -> Nullable<Timestamptz>,
        completed_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        progress_pct -> Int4,
        progress_message -> Text,
        commit_sha -> Nullable<Text>,
        force_index -> Bool,
        url_hash -> Nullable<Text>,
    }
}

diesel::table! {
    index_rules (id) {
        id -> Uuid,
        repo_url -> Text,
        path_regex -> Text,
        strategy -> Text,
        description -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    reports (id) {
        id -> Uuid,
        reporter_user_id -> Uuid,
        target_type -> Text,
        target_id -> Uuid,
        reason -> Text,
        description -> Text,
        status -> Text,
        reviewed_by_user_id -> Nullable<Uuid>,
        reviewed_at -> Nullable<Timestamptz>,
        notes -> Nullable<Text>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    repos (id) {
        id -> Uuid,
        sign -> Text,
        name -> Text,
        description -> Text,
        git_url -> Text,
        git_rev -> Nullable<Text>,
        git_branch -> Nullable<Text>,
        license -> Nullable<Text>,
        visibility -> Text,
        verified -> Bool,
        metadata -> Jsonb,
        keywords -> Array<Nullable<Text>>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        last_indexed_at -> Nullable<Timestamptz>,
    }
}

diesel::table! {
    security_scans (id) {
        id -> Uuid,
        target_type -> Text,
        target_id -> Uuid,
        scan_module -> Text,
        result -> Text,
        severity -> Nullable<Text>,
        details -> Jsonb,
        scanned_by_user_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
        version_id -> Nullable<Uuid>,
        commit_sha -> Nullable<Text>,
    }
}

diesel::table! {
    site_admins (id) {
        id -> Uuid,
        user_id -> Uuid,
        granted_by_user_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    skill_blocks (id) {
        id -> Uuid,
        user_id -> Uuid,
        repo_id -> Uuid,
        flock_id -> Uuid,
        skill_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    skill_comments (id) {
        id -> Uuid,
        user_id -> Uuid,
        repo_id -> Uuid,
        flock_id -> Uuid,
        skill_id -> Nullable<Uuid>,
        body -> Text,
        soft_deleted_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    skill_installs (id) {
        id -> Uuid,
        skill_id -> Uuid,
        flock_id -> Uuid,
        user_id -> Nullable<Uuid>,
        client_type -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    skill_ratings (id) {
        id -> Uuid,
        repo_id -> Uuid,
        flock_id -> Uuid,
        user_id -> Uuid,
        score -> Int2,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::table! {
    skill_stars (id) {
        id -> Uuid,
        user_id -> Uuid,
        repo_id -> Uuid,
        flock_id -> Uuid,
        skill_id -> Nullable<Uuid>,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    skill_versions (id) {
        id -> Uuid,
        repo_id -> Uuid,
        flock_id -> Nullable<Uuid>,
        skill_id -> Nullable<Uuid>,
        git_rev -> Nullable<Text>,
        git_branch -> Nullable<Text>,
        version -> Nullable<Text>,
        changelog -> Text,
        tags -> Array<Nullable<Text>>,
        files -> Jsonb,
        parsed_metadata -> Jsonb,
        search_document -> Text,
        fingerprint -> Text,
        created_by -> Uuid,
        created_at -> Timestamptz,
        soft_deleted_at -> Nullable<Timestamptz>,
        scan_summary -> Nullable<Jsonb>,
    }
}

diesel::table! {
    skills (id) {
        id -> Uuid,
        sign -> Text,
        slug -> Text,
        name -> Text,
        path -> Text,
        keywords -> Array<Nullable<Text>>,
        description -> Nullable<Text>,
        repo_id -> Uuid,
        flock_id -> Uuid,
        version -> Nullable<Text>,
        status -> Text,
        license -> Nullable<Text>,
        source -> Jsonb,
        metadata -> Jsonb,
        entry_data -> Nullable<Jsonb>,
        runtime_data -> Nullable<Jsonb>,
        security_status -> Text,
        latest_version_id -> Nullable<Uuid>,
        tags -> Jsonb,
        moderation_status -> Text,
        highlighted -> Bool,
        official -> Bool,
        deprecated -> Bool,
        suspicious -> Bool,
        stats_downloads -> Int8,
        stats_stars -> Int8,
        stats_versions -> Int8,
        stats_comments -> Int8,
        soft_deleted_at -> Nullable<Timestamptz>,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
        stats_installs -> Int8,
        stats_unique_users -> Int8,
    }
}

diesel::table! {
    browse_histories (id) {
        id -> Uuid,
        user_id -> Uuid,
        resource_type -> Text,
        resource_id -> Uuid,
        viewed_at -> Timestamptz,
    }
}

diesel::table! {
    user_tokens (id) {
        id -> Uuid,
        user_id -> Uuid,
        name -> Text,
        token -> Text,
        created_at -> Timestamptz,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        handle -> Text,
        display_name -> Nullable<Text>,
        bio -> Nullable<Text>,
        avatar_url -> Nullable<Text>,
        github_user_id -> Nullable<Text>,
        github_login -> Nullable<Text>,
        role -> Text,
        created_at -> Timestamptz,
        updated_at -> Timestamptz,
    }
}

diesel::joinable!(audit_logs -> users (actor_user_id));
diesel::joinable!(flocks -> repos (repo_id));
diesel::joinable!(flocks -> users (imported_by_user_id));
diesel::joinable!(index_jobs -> users (requested_by_user_id));
diesel::joinable!(security_scans -> skill_versions (version_id));
diesel::joinable!(security_scans -> users (scanned_by_user_id));
diesel::joinable!(skill_blocks -> flocks (flock_id));
diesel::joinable!(skill_blocks -> repos (repo_id));
diesel::joinable!(skill_blocks -> skills (skill_id));
diesel::joinable!(skill_blocks -> users (user_id));
diesel::joinable!(skill_comments -> flocks (flock_id));
diesel::joinable!(skill_comments -> repos (repo_id));
diesel::joinable!(skill_comments -> skills (skill_id));
diesel::joinable!(skill_comments -> users (user_id));
diesel::joinable!(skill_installs -> flocks (flock_id));
diesel::joinable!(skill_installs -> skills (skill_id));
diesel::joinable!(skill_installs -> users (user_id));
diesel::joinable!(skill_ratings -> flocks (flock_id));
diesel::joinable!(skill_ratings -> repos (repo_id));
diesel::joinable!(skill_ratings -> users (user_id));
diesel::joinable!(skill_stars -> flocks (flock_id));
diesel::joinable!(skill_stars -> repos (repo_id));
diesel::joinable!(skill_stars -> skills (skill_id));
diesel::joinable!(skill_stars -> users (user_id));
diesel::joinable!(skill_versions -> flocks (flock_id));
diesel::joinable!(skill_versions -> repos (repo_id));
diesel::joinable!(skill_versions -> skills (skill_id));
diesel::joinable!(skill_versions -> users (created_by));
diesel::joinable!(skills -> flocks (flock_id));
diesel::joinable!(skills -> repos (repo_id));
diesel::joinable!(browse_histories -> users (user_id));
diesel::joinable!(user_tokens -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    ai_request_cache,
    ai_usage_logs,
    audit_logs,
    flocks,
    index_jobs,
    index_rules,
    reports,
    repos,
    security_scans,
    site_admins,
    skill_blocks,
    skill_comments,
    skill_installs,
    skill_ratings,
    skill_stars,
    skill_versions,
    skills,
    browse_histories,
    user_tokens,
    users,
);
