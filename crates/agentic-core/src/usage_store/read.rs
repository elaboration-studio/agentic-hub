use super::*;

impl UsageStore {
    /// Query terminal usage counts for the provided capability ids.
    pub fn query_stats(&self, capability_ids: &[String]) -> Result<Vec<UsageStats>> {
        if capability_ids.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.connect()?;
        let live_ids: HashSet<&str> = capability_ids.iter().map(String::as_str).collect();
        let mut stats: HashMap<String, UsageStats> = capability_ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    UsageStats {
                        capability_id: id.clone(),
                        ..UsageStats::default()
                    },
                )
            })
            .collect();

        let terminal_filter = terminal_sql_filter();
        let mut stmt = conn.prepare(&format!(
            r#"
            SELECT capability_id,
                   source_tool,
                   COUNT(*) AS execution_count,
                   SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END) AS success_count,
                   SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END) AS failure_count,
                   MAX(timestamp) AS last_used_at
            FROM usage_events
            WHERE capability_id IS NOT NULL
              AND event_type IN ({terminal_filter})
            GROUP BY capability_id, source_tool
            "#
        ))?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                UsageToolBucket {
                    source_tool: row.get(1)?,
                    execution_count: row.get::<_, i64>(2)? as u32,
                    success_count: row.get::<_, i64>(3)? as u32,
                    failure_count: row.get::<_, i64>(4)? as u32,
                    last_used_at: row.get(5)?,
                },
            ))
        })?;

        for row in rows {
            let (capability_id, bucket) = row?;
            if !live_ids.contains(capability_id.as_str()) {
                continue;
            }
            if let Some(stat) = stats.get_mut(&capability_id) {
                stat.execution_count += bucket.execution_count;
                stat.success_count += bucket.success_count;
                stat.failure_count += bucket.failure_count;
                stat.last_used_at =
                    max_timestamp(stat.last_used_at.take(), bucket.last_used_at.clone());
                stat.tool_buckets.push(bucket);
            }
        }

        let mut out: Vec<UsageStats> = stats
            .into_values()
            .filter(|s| s.execution_count > 0)
            .collect();
        out.sort_by(|a, b| a.capability_id.cmp(&b.capability_id));
        for stat in &mut out {
            stat.tool_buckets
                .sort_by(|a, b| a.source_tool.cmp(&b.source_tool));
        }
        Ok(out)
    }

    /// Query usage for Manager items using scoped persisted identities.
    pub fn query_stats_for_items(&self, items: &[CapabilityItem]) -> Result<Vec<UsageStats>> {
        let queries: Vec<UsageCapabilityQuery> = items
            .iter()
            .filter(|item| is_countable(item))
            .map(UsageCapabilityQuery::from_item)
            .collect();
        if queries.is_empty() {
            return Ok(Vec::new());
        }
        let mut by_identity: HashMap<String, UsageStats> = queries
            .iter()
            .map(|query| {
                (
                    usage_identity_key(
                        query.capability_scope,
                        query.workspace_root.as_deref(),
                        &query.capability_id,
                    ),
                    UsageStats {
                        capability_id: query.external_id.clone(),
                        ..UsageStats::default()
                    },
                )
            })
            .collect();
        let conn = self.connect()?;
        let terminal_filter = terminal_sql_filter();
        let mut stmt = conn.prepare(&format!(
            r#"
            SELECT capability_scope, workspace_root, capability_id, source_tool,
                   COUNT(*),
                   SUM(CASE WHEN success = 1 THEN 1 ELSE 0 END),
                   SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END),
                   MAX(timestamp)
            FROM usage_events
            WHERE capability_id IS NOT NULL
              AND event_type IN ({terminal_filter})
            GROUP BY capability_scope, workspace_root, capability_id, source_tool
            "#
        ))?;
        let rows = stmt.query_map([], |row| {
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
        for row in rows {
            let (scope, workspace_root, capability_id, bucket) = row?;
            let scope = if scope == CapabilityScope::Workspace.as_str() {
                CapabilityScope::Workspace
            } else {
                CapabilityScope::Global
            };
            let key = usage_identity_key(scope, workspace_root.as_deref(), &capability_id);
            let Some(stat) = by_identity.get_mut(&key) else {
                continue;
            };
            stat.execution_count += bucket.execution_count;
            stat.success_count += bucket.success_count;
            stat.failure_count += bucket.failure_count;
            stat.last_used_at =
                max_timestamp(stat.last_used_at.take(), bucket.last_used_at.clone());
            stat.tool_buckets.push(bucket);
        }
        let mut out: Vec<UsageStats> = by_identity
            .into_values()
            .filter(|stat| stat.execution_count > 0)
            .collect();
        out.sort_by(|left, right| left.capability_id.cmp(&right.capability_id));
        for stat in &mut out {
            stat.tool_buckets
                .sort_by(|left, right| left.source_tool.cmp(&right.source_tool));
        }
        Ok(out)
    }

    pub fn event_count(&self) -> Result<u32> {
        let conn = self.connect()?;
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM usage_events", [], |row| row.get(0))?;
        Ok(count as u32)
    }

    pub fn resolved_event_count(&self) -> Result<u32> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NOT NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    pub fn unresolved_event_count(&self) -> Result<u32> {
        let conn = self.connect()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NULL",
            [],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }

    pub fn tool_diagnostics(&self) -> Result<Vec<UsageToolDiagnosticSummary>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT source_tool,
                   MAX(timestamp),
                   SUM(CASE WHEN capability_id IS NOT NULL THEN 1 ELSE 0 END),
                   SUM(CASE WHEN capability_id IS NULL THEN 1 ELSE 0 END)
            FROM usage_events
            GROUP BY source_tool
            ORDER BY source_tool
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UsageToolDiagnosticSummary {
                source_tool: row.get(0)?,
                last_captured_at: row.get(1)?,
                resolved_event_count: row.get::<_, i64>(2)? as u32,
                unresolved_event_count: row.get::<_, i64>(3)? as u32,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    /// Re-run skill resolution for stored rows that have a `skill_name` but no
    /// `capability_id`. Returns the number of rows updated.
    pub fn re_resolve_events(&self, items: &[CapabilityItem]) -> Result<u32> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT dedupe_hash, skill_name
            FROM usage_events
            WHERE capability_id IS NULL
              AND skill_name IS NOT NULL
              AND trim(skill_name) != ''
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut updated = 0_u32;
        for row in rows {
            let (dedupe_hash, skill_name) = row?;
            let Some(capability_id) = resolve_capability_id(items, None, Some(&skill_name)) else {
                continue;
            };
            let changed = conn.execute(
                "UPDATE usage_events SET capability_id = ?1 WHERE dedupe_hash = ?2 AND capability_id IS NULL",
                params![capability_id, dedupe_hash],
            )?;
            updated += changed as u32;
        }
        Ok(updated)
    }

    pub fn unresolved_references(&self) -> Result<Vec<UnresolvedUsageReference>> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            r#"
            SELECT dedupe_hash, source_tool, skill_name,
                   COALESCE(workspace_root, workspace)
            FROM usage_events
            WHERE capability_id IS NULL
              AND skill_name IS NOT NULL
              AND trim(skill_name) != ''
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UnresolvedUsageReference {
                dedupe_hash: row.get(0)?,
                source_tool: row.get(1)?,
                skill_name: row.get(2)?,
                workspace_root: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn reconcile_resolution(
        &self,
        dedupe_hash: &str,
        capability_id: &str,
        scope: CapabilityScope,
        workspace_root: Option<&str>,
        relative_path: Option<&str>,
    ) -> Result<bool> {
        let conn = self.connect()?;
        let changed = conn.execute(
            r#"
            UPDATE usage_events
            SET capability_id = ?1,
                capability_scope = ?2,
                workspace_root = ?3,
                capability_relative_path = ?4
            WHERE dedupe_hash = ?5
              AND capability_id IS NULL
            "#,
            params![
                capability_id,
                scope.as_str(),
                workspace_root,
                relative_path,
                dedupe_hash,
            ],
        )?;
        Ok(changed == 1)
    }
    /// Aggregate dashboard metrics for the Statistics page.
    pub fn query_dashboard(
        &self,
        items: &[CapabilityItem],
        range: UsageDateRange,
    ) -> Result<UsageDashboard> {
        let conn = self.connect()?;
        let terminal_filter = terminal_sql_filter();
        let range_clause = range_sql_clause(range);
        let countable: Vec<&CapabilityItem> =
            items.iter().filter(|item| is_countable(item)).collect();
        let item_map: HashMap<String, &CapabilityItem> = countable
            .iter()
            .map(|item| (item_usage_identity_key(item), *item))
            .collect();

        let total_events: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM usage_events WHERE 1=1 {range_clause}"),
            [],
            |row| row.get(0),
        )?;
        let terminal_events: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM usage_events WHERE event_type IN ({terminal_filter}) {range_clause}"
            ),
            [],
            |row| row.get(0),
        )?;
        let resolved_events: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NOT NULL {range_clause}"
            ),
            [],
            |row| row.get(0),
        )?;
        let unresolved_events: i64 = conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM usage_events WHERE capability_id IS NULL {range_clause}"
            ),
            [],
            |row| row.get(0),
        )?;

        let mut by_kind = query_kind_buckets(&conn, &terminal_filter, &range_clause)?;
        by_kind.retain(|bucket| bucket.kind != "other");
        let mut by_source_tool = query_source_buckets(&conn, &terminal_filter, &range_clause)?;
        let mut by_day = query_day_buckets(&conn, &terminal_filter, &range_clause)?;
        let mut by_workspace = query_workspace_buckets(&conn, &terminal_filter, &range_clause)?;

        let used_ids = query_used_capability_identities(&conn, &terminal_filter, &range_clause)?;
        let traced_capabilities = used_ids
            .iter()
            .filter(|id| item_map.contains_key(*id))
            .count() as u32;
        let unused_countable = countable
            .iter()
            .filter(|item| !used_ids.contains(&item_usage_identity_key(item)))
            .count() as u32;

        let top_capabilities =
            query_top_capabilities(&conn, &terminal_filter, &range_clause, &item_map)?;
        let today_top_capabilities =
            query_top_capabilities(&conn, &terminal_filter, TODAY_CLAUSE, &item_map)?;
        let mut unused_capabilities: Vec<UsageUnusedRow> = countable
            .iter()
            .filter(|item| !used_ids.contains(&item_usage_identity_key(item)))
            .map(|item| UsageUnusedRow {
                capability_id: item.id.clone(),
                name: item.name.clone(),
                kind: item.kind,
                source_label: item.source_label.clone(),
                relative_path: item.relative_path.to_string_lossy().into_owned(),
            })
            .collect();
        unused_capabilities.sort_by(|a, b| {
            a.kind
                .dir_name()
                .cmp(b.kind.dir_name())
                .then(a.name.cmp(&b.name))
        });

        sort_kind_buckets(&mut by_kind);
        by_source_tool.sort_by_key(|b| std::cmp::Reverse(b.execution_count));
        by_day.sort_by(|a, b| a.day.cmp(&b.day));
        by_workspace.sort_by_key(|b| std::cmp::Reverse(b.execution_count));

        Ok(UsageDashboard {
            overview: UsageDashboardOverview {
                total_events: total_events as u32,
                terminal_events: terminal_events as u32,
                resolved_events: resolved_events as u32,
                unresolved_events: unresolved_events as u32,
                traced_capabilities,
                installed_countable: countable.len() as u32,
                unused_countable,
            },
            by_kind,
            by_source_tool,
            by_day,
            top_capabilities,
            today_top_capabilities,
            unused_capabilities,
            by_workspace,
        })
    }
}
