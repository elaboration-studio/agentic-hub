use super::*;

pub(super) fn is_countable(item: &CapabilityItem) -> bool {
    matches!(
        item.kind,
        CapabilityKind::Skill | CapabilityKind::Command | CapabilityKind::Agent
    )
}

pub(super) fn range_sql_clause(range: UsageDateRange) -> String {
    match range {
        UsageDateRange::Today => " AND timestamp >= datetime('now', '-1 days')".to_string(),
        UsageDateRange::Last7Days => " AND timestamp >= datetime('now', '-7 days')".to_string(),
        UsageDateRange::Last30Days => " AND timestamp >= datetime('now', '-30 days')".to_string(),
        UsageDateRange::Last90Days => " AND timestamp >= datetime('now', '-90 days')".to_string(),
        UsageDateRange::AllTime => String::new(),
    }
}

pub(super) fn query_kind_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageKindBucket>> {
    let sql = format!(
        r#"
        SELECT CASE
                 WHEN capability_id LIKE 'skill:%' THEN 'skill'
                 WHEN capability_id LIKE 'command:%' THEN 'command'
                 WHEN capability_id LIKE 'agent:%' THEN 'agent'
                 ELSE 'other'
               END AS kind,
               COUNT(*) AS execution_count
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY kind
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageKindBucket {
            kind: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn sort_kind_buckets(buckets: &mut [UsageKindBucket]) {
    const ORDER: [&str; 3] = ["skill", "command", "agent"];
    buckets.sort_by(|a, b| {
        let ai = ORDER.iter().position(|k| *k == a.kind).unwrap_or(99);
        let bi = ORDER.iter().position(|k| *k == b.kind).unwrap_or(99);
        ai.cmp(&bi)
    });
}

pub(super) fn query_source_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageSourceBucket>> {
    let sql = format!(
        r#"
        SELECT source_tool, COUNT(*) AS execution_count
        FROM usage_events
        WHERE event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY source_tool
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageSourceBucket {
            source_tool: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn query_day_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageDayBucket>> {
    let sql = format!(
        r#"
        SELECT date(timestamp) AS day, COUNT(*) AS execution_count
        FROM usage_events
        WHERE event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY day
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageDayBucket {
            day: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn query_workspace_buckets(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<Vec<UsageWorkspaceBucket>> {
    let sql = format!(
        r#"
        SELECT COALESCE(workspace_root, workspace), COUNT(*) AS execution_count
        FROM usage_events
        WHERE COALESCE(workspace_root, workspace) IS NOT NULL
          AND trim(COALESCE(workspace_root, workspace)) != ''
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY COALESCE(workspace_root, workspace)
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        Ok(UsageWorkspaceBucket {
            workspace: row.get(0)?,
            execution_count: row.get::<_, i64>(1)? as u32,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn query_used_capability_identities(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
) -> Result<HashSet<String>> {
    let sql = format!(
        r#"
        SELECT DISTINCT capability_scope, workspace_root, capability_id
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |row| {
        let scope: String = row.get(0)?;
        let workspace_root: Option<String> = row.get(1)?;
        let capability_id: String = row.get(2)?;
        Ok(usage_identity_key(
            parse_scope(&scope),
            workspace_root.as_deref(),
            &capability_id,
        ))
    })?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .collect())
}

pub(super) fn query_top_capabilities(
    conn: &Connection,
    terminal_filter: &str,
    range_clause: &str,
    item_map: &HashMap<String, &CapabilityItem>,
) -> Result<Vec<UsageTopRow>> {
    let sql = format!(
        r#"
        SELECT capability_scope,
               workspace_root,
               capability_id,
               MAX(skill_name),
               MAX(capability_relative_path),
               COUNT(*) AS execution_count,
               MAX(timestamp) AS last_used_at
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY capability_scope, workspace_root, capability_id
        ORDER BY execution_count DESC, capability_scope, workspace_root, capability_id
        LIMIT 15
        "#
    );
    let mut stmt = conn.prepare(&sql)?;
    let summary_rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, i64>(5)? as u32,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;

    let bucket_sql = format!(
        r#"
        SELECT capability_scope,
               workspace_root,
               capability_id,
               source_tool,
               COUNT(*) AS execution_count,
               SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) AS success_count,
               SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END) AS failure_count,
               MAX(timestamp) AS last_used_at
        FROM usage_events
        WHERE capability_id IS NOT NULL
          AND event_type IN ({terminal_filter})
          {range_clause}
        GROUP BY capability_scope, workspace_root, capability_id, source_tool
        "#
    );
    let mut bucket_stmt = conn.prepare(&bucket_sql)?;
    let bucket_rows = bucket_stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            UsageToolBucket {
                source_tool: row.get(3)?,
                execution_count: row.get::<_, i64>(4)? as u32,
                success_count: row.get::<_, i64>(5)? as u32,
                failure_count: row.get::<_, i64>(6)? as u32,
                last_used_at: row.get(7)?,
            },
        ))
    })?;
    let mut buckets: HashMap<String, Vec<UsageToolBucket>> = HashMap::new();
    for row in bucket_rows {
        let (scope, workspace_root, capability_id, bucket) = row?;
        let key = usage_identity_key(
            parse_scope(&scope),
            workspace_root.as_deref(),
            &capability_id,
        );
        buckets.entry(key).or_default().push(bucket);
    }
    for list in buckets.values_mut() {
        list.sort_by(|a, b| a.source_tool.cmp(&b.source_tool));
    }

    let mut out = Vec::new();
    for row in summary_rows {
        let (
            scope,
            workspace_root,
            capability_id,
            skill_name,
            stored_relative_path,
            execution_count,
            last_used_at,
        ) = row?;
        let capability_scope = parse_scope(&scope);
        let identity =
            usage_identity_key(capability_scope, workspace_root.as_deref(), &capability_id);
        let item = item_map.get(&identity).copied();
        let Some(kind) = item
            .map(|item| item.kind)
            .or_else(|| capability_kind_from_id(&capability_id))
        else {
            continue;
        };
        out.push(UsageTopRow {
            capability_id: item.map_or_else(|| capability_id.clone(), |item| item.id.clone()),
            name: item.map_or_else(
                || skill_name.unwrap_or_else(|| capability_name_from_id(&capability_id)),
                |item| item.name.clone(),
            ),
            kind,
            source_label: item.map_or_else(
                || {
                    if capability_scope == CapabilityScope::Workspace {
                        "Workspace".to_string()
                    } else {
                        "Local usage history".to_string()
                    }
                },
                |item| item.source_label.clone(),
            ),
            relative_path: item.map_or_else(
                || stored_relative_path.unwrap_or_default(),
                |item| item.relative_path.to_string_lossy().into_owned(),
            ),
            capability_scope,
            workspace_root,
            execution_count,
            last_used_at,
            tool_buckets: buckets.remove(&identity).unwrap_or_default(),
        });
    }
    Ok(out)
}
