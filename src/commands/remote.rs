use anyhow::{Context, Result};
use colored::*;
use lightmem::{CliErrorCard, CliView, EntityFilter, PaginatedMemories, RemoteClient};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use super::context::AlreadyHandledError;
use crate::{parse_tag_list, BundleCommand, Commands};

fn encode_attachments(files: &[PathBuf]) -> Result<Option<Vec<lightmem::AttachmentPayload>>> {
    if files.is_empty() {
        return Ok(None);
    }
    let mut payloads = Vec::with_capacity(files.len());
    for fpath in files {
        let data = std::fs::read(fpath)
            .with_context(|| format!("Failed to read attachment file: {}", fpath.display()))?;
        let fname = fpath
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();
        let (mime, _) = lightmem::detect_mime_and_kind(&fname, &data);
        payloads.push(lightmem::AttachmentPayload {
            name: fname,
            mime: Some(mime),
            data_base64: lightmem::encode_base64(&data),
        });
    }
    Ok(Some(payloads))
}

fn format_entity_filter(entity: Option<&EntityFilter>) -> Option<String> {
    entity.map(|e| match &e.val {
        Some(v) => format!("{}={}", e.name, v),
        None => e.name.clone(),
    })
}

pub fn handle_remote_command(
    client: &RemoteClient,
    command: &Commands,
    session: Option<&str>,
    json: bool,
) -> Result<bool> {
    let clean_session = session
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string);
    match command {
        Commands::Remember {
            content,
            files,
            extract,
            reranker,
            category,
            title,
            tags,
            confidence,
            ttl,
            supersede,
            provenance,
            no_entities,
        } => {
            anyhow::ensure!(
                !extract && reranker.is_none(),
                "Remote remember does not support --extract or --reranker. Configure extraction on the server or omit these options."
            );
            let resolved_content = if content.as_deref() == Some("-")
                || (content.is_none() && files.is_empty() && !io::stdin().is_terminal())
            {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                let trimmed = buf.trim().to_string();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed)
                }
            } else {
                content.clone()
            };
            let attachment_payloads = encode_attachments(files)?;
            let req = lightmem::RememberRequest {
                content: resolved_content,
                category: *category,
                tags: tags
                    .as_ref()
                    .map(|t| parse_tag_list(std::slice::from_ref(t))),
                source: provenance.clone(),
                title: title.clone(),
                ttl: ttl.map(|d| format!("{}ms", d.num_milliseconds())),
                confidence: Some(*confidence),
                supersede: *supersede,
                no_entities: Some(*no_entities),
                attachments: attachment_payloads,
            };
            let response = client.remember_detailed(&req)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&response)?);
            } else {
                CliView::render_remembered(&response.memory, Path::new(&client.base_url), false)?;
                for warning in response.warnings {
                    eprintln!("Warning: {}", warning);
                }
            }
            Ok(true)
        }

        Commands::Recall {
            query,
            image,
            category,
            tag,
            entity,
            media,
            as_of,
            date,
            limit,
            min_similarity,
            precision,
            multi_hop,
            save_to,
            path,
            open,
        } => {
            anyhow::ensure!(
                image.is_none() && save_to.is_none() && !path && !open,
                "Remote recall does not support --image, --save-to, --path, or --open. Use local mode to download attachments."
            );
            let tags = parse_tag_list(tag);
            let entity_str = format_entity_filter(entity.as_ref());
            if *precision {
                anyhow::ensure!(
                    media.is_none() && !multi_hop && min_similarity.is_none(),
                    "Precision recall returns an answer; media, multi-hop, and similarity filters are not supported. Use recall without --precision for filtered results."
                );
                let answer = match client.answer(&lightmem::AnswerRequest {
                    question: query.clone(),
                    category: *category,
                    tags,
                    session: clean_session,
                    entity: entity_str,
                    as_of: *as_of,
                    date: *date,
                    limit: *limit,
                    ..Default::default()
                }) {
                    Ok(ans) => ans,
                    Err(err) => {
                        let msg = err.to_string();
                        CliView::render_llm_or_error(&err, json);
                        return Err(AlreadyHandledError::new(msg).into());
                    }
                };
                CliView::render_answer(&answer, json)?;
                return Ok(true);
            }
            let req = lightmem::RecallRequest {
                query: query.clone(),
                category: *category,
                tags,
                session: clean_session,
                entity: entity_str,
                media: media.clone(),
                as_of: *as_of,
                date: *date,
                limit: *limit,
                min_similarity: *min_similarity,
                precision: *precision,
                multi_hop: *multi_hop,
            };
            let results = client.recall(&req)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else {
                CliView::render_recall(query, &results, Path::new(&client.base_url), false)?;
            }
            Ok(true)
        }

        Commands::Answer {
            question,
            precision,
            reranker,
            category,
            tag,
            entity,
            as_of,
            date,
            limit,
        } => {
            let req = lightmem::AnswerRequest {
                question: question.clone(),
                precision: *precision,
                reranker: reranker.clone(),
                category: *category,
                tags: parse_tag_list(tag),
                session: clean_session,
                entity: format_entity_filter(entity.as_ref()),
                as_of: *as_of,
                date: *date,
                limit: *limit,
            };
            let result = match client.answer(&req) {
                Ok(ans) => ans,
                Err(err) => {
                    let msg = err.to_string();
                    CliView::render_llm_or_error(&err, json);
                    return Err(AlreadyHandledError::new(msg).into());
                }
            };
            CliView::render_answer(&result, json)?;
            Ok(true)
        }

        Commands::List {
            category,
            tag,
            entity,
            media,
            status,
            sort,
            as_of,
            date,
            limit,
            offset,
            page,
            save_to,
            path,
            open,
        } => {
            anyhow::ensure!(
                save_to.is_none() && !path && !open,
                "Remote list does not support --save-to, --path, or --open. Use local mode to download attachments."
            );
            let tags = parse_tag_list(tag);
            let entity_str = format_entity_filter(entity.as_ref());
            let paginated = client.list_filtered(
                *limit,
                *offset,
                *page,
                *category,
                status.into_memory_status(),
                matches!(status, crate::ListStatusFilter::All),
                media.clone(),
                *date,
                *as_of,
                &tags,
                clean_session.as_deref(),
                entity_str.as_deref(),
                *sort,
            )?;
            CliView::render_paginated_list(&paginated, Path::new(&client.base_url), json)?;
            Ok(true)
        }

        Commands::Inspect {
            terms,
            save_to,
            path,
            open,
        } => {
            anyhow::ensure!(
                save_to.is_none() && !path && !open,
                "Remote inspect does not support --save-to, --path, or --open. Use local mode to download attachments."
            );
            let mut inspected = Vec::new();
            for term in terms {
                if let Some(record) = client.get(term)? {
                    let links = client.links(&record.id)?;
                    if json {
                        inspected.push(serde_json::json!({"memory": record, "links": links}));
                    } else {
                        CliView::render_inspect(
                            &record,
                            &links,
                            Path::new(&client.base_url),
                            false,
                        )?;
                    }
                }
            }
            if inspected.is_empty() {
                if json {
                    if terms.len() > 1 {
                        println!("[]");
                    } else {
                        println!("null");
                    }
                } else {
                    eprintln!("Memory not found: {}", terms.join(", "));
                }
                return Err(AlreadyHandledError::new(format!(
                    "Memory not found: {}",
                    terms.join(", ")
                ))
                .into());
            }
            if json {
                let value = if terms.len() > 1 {
                    serde_json::json!(inspected)
                } else {
                    inspected
                        .into_iter()
                        .next()
                        .unwrap_or(serde_json::Value::Null)
                };
                println!("{}", serde_json::to_string_pretty(&value)?);
            }
            Ok(true)
        }

        Commands::Update {
            id,
            content,
            title,
            category,
            tags,
            add_tags,
            remove_tags,
            confidence,
            ttl,
            clear_ttl,
            provenance,
            files,
            refresh_entities,
        } => {
            let attachments = encode_attachments(files)?;
            let req = lightmem::UpdateRequest {
                id: id.clone(),
                content: content.clone(),
                title: title.clone(),
                category: *category,
                tags: tags
                    .as_ref()
                    .map(|t| parse_tag_list(std::slice::from_ref(t))),
                add_tags: parse_tag_list(add_tags),
                remove_tags: parse_tag_list(remove_tags),
                confidence: *confidence,
                provenance: provenance.clone(),
                ttl: ttl.map(|d| format!("{}ms", d.num_milliseconds())),
                clear_ttl: *clear_ttl,
                attachments,
                refresh_entities: *refresh_entities,
            };
            let updated = client.update(&req)?;
            let links = client.links(&updated.id)?;
            if !json {
                println!(
                    "\n  {} {}",
                    CliView::emerald_bold("✓"),
                    CliView::emerald_bold("Memory updated in-place")
                );
            }
            CliView::render_inspect(&updated, &links, Path::new(&client.base_url), json)?;
            Ok(true)
        }

        Commands::Deduplicate => {
            let merged = client.deduplicate()?;
            CliView::render_dedup(merged, json)?;
            Ok(true)
        }

        Commands::Forget {
            ids,
            all,
            hard,
            yes,
        } => {
            if *all {
                if !*yes {
                    if !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
                        let action = if *hard {
                            "permanently delete"
                        } else {
                            "expire"
                        };
                        eprint!(
                            "Are you sure you want to {} all memories on remote server '{}'? [y/N]: ",
                            action, client.base_url
                        );
                        std::io::stderr().flush()?;
                        let mut input = String::new();
                        std::io::stdin().read_line(&mut input)?;
                        if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                            eprintln!("Operation cancelled.");
                            return Ok(true);
                        }
                    } else {
                        anyhow::bail!("Refusing to forget all remote memories without confirmation. Pass --yes (or -y) to proceed non-interactively.");
                    }
                }
                let deleted = client.forget_all(*hard)?;
                CliView::render_clear(deleted, *hard, Path::new(&client.base_url), json)?;
            } else if ids.len() == 1 {
                let target_id = &ids[0];
                let ok = client.forget(Some(target_id), false, *hard)?;
                CliView::render_forget(target_id, ok, *hard, json)?;
                if !ok {
                    return Err(AlreadyHandledError::new(format!(
                        "Memory '{}' not found",
                        target_id
                    ))
                    .into());
                }
            } else if ids.len() > 1 {
                let mut results = Vec::with_capacity(ids.len());
                let mut all_ok = true;
                for target_id in ids {
                    let ok = client.forget(Some(target_id), false, *hard)?;
                    if !ok {
                        all_ok = false;
                    }
                    if json {
                        results.push(serde_json::json!({
                            "id": target_id,
                            "success": ok,
                            "hard_deleted": *hard,
                        }));
                    } else {
                        CliView::render_forget(target_id, ok, *hard, false)?;
                    }
                }
                if json {
                    println!("{}", serde_json::to_string_pretty(&results)?);
                }
                if !all_ok {
                    return Err(
                        AlreadyHandledError::new("One or more memories were not found").into(),
                    );
                }
            } else {
                anyhow::bail!("Provide a memory <ID> or pass '--all' to forget all memories.");
            }
            Ok(true)
        }

        Commands::Clear { soft, yes } => {
            if !*yes {
                if !json && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
                    let action = if !*soft {
                        "permanently clear"
                    } else {
                        "soft-retire"
                    };
                    eprint!(
                        "Are you sure you want to {} all memories on remote server '{}'? [y/N]: ",
                        action, client.base_url
                    );
                    std::io::stderr().flush()?;
                    let mut input = String::new();
                    std::io::stdin().read_line(&mut input)?;
                    if !matches!(input.trim().to_lowercase().as_str(), "y" | "yes") {
                        eprintln!("Operation cancelled.");
                        return Ok(true);
                    }
                } else {
                    anyhow::bail!("Refusing to clear remote vault without confirmation. Pass --yes (or -y) to proceed non-interactively.");
                }
            }
            let removed = client.clear(*soft)?;
            CliView::render_clear(removed, !*soft, Path::new(&client.base_url), json)?;
            Ok(true)
        }

        Commands::Stats => {
            let stats = client.stats()?;
            CliView::render_stats(&stats, Path::new(&client.base_url), json)?;
            Ok(true)
        }

        Commands::Link {
            source,
            target,
            relation,
            weight,
        } => {
            let link = client.link(source, target, Some(relation), Some(*weight))?;
            CliView::render_link(
                &link.source_id,
                &link.target_id,
                &link.relation,
                link.weight,
                json,
            )?;
            Ok(true)
        }

        Commands::Unlink {
            source,
            target,
            relation,
        } => {
            let unlinked = client.unlink(source, target, relation.as_deref())?;
            CliView::render_unlink(source, target, relation.as_deref(), unlinked, json)?;
            Ok(true)
        }

        Commands::Links { id } => {
            let links = client.links(id)?;
            CliView::render_links_list(id, &links, json)?;
            Ok(true)
        }

        Commands::Tags { query } => {
            let tags = client.tags(Some(query.as_str()))?;
            CliView::render_tags(&tags, json)?;
            Ok(true)
        }

        Commands::Entities {
            name,
            val,
            memories,
        } => {
            let (parsed_name, parsed_val) = match (name.clone(), val.clone()) {
                (Some(raw), explicit_val) => {
                    if let Some(v) = explicit_val {
                        (Some(raw), Some(v))
                    } else if let Some((k, v)) = raw.split_once('=') {
                        (Some(k.trim().to_string()), Some(v.trim().to_string()))
                    } else {
                        (Some(raw), None)
                    }
                }
                (None, Some(v)) => {
                    anyhow::bail!("--val '{}' requires an entity name argument", v);
                }
                (None, None) => (None, None),
            };
            if parsed_val.is_some() || *memories {
                let target_name = parsed_name
                    .as_deref()
                    .context("Provide an entity name when using --memories or --val")?;
                let records = client.find_by_entity(target_name, parsed_val.as_deref())?;
                let total = records.len();
                let paginated = PaginatedMemories {
                    items: records,
                    total,
                    limit: total.max(1),
                    offset: 0,
                    page: 1,
                    total_pages: 1,
                    has_more: false,
                };
                CliView::render_paginated_list(&paginated, Path::new(&client.base_url), json)?;
            } else {
                let list = client.entities(parsed_name.as_deref())?;
                CliView::render_entities(&list, json)?;
            }
            Ok(true)
        }

        Commands::Related { id, hops } => {
            let results = client.related(id, *hops)?;
            CliView::render_related(id, &results, json)?;
            Ok(true)
        }

        Commands::Graph { focus, hops } => {
            let snapshot = client.graph(focus.as_deref(), Some(*hops))?;
            CliView::render_graph(&snapshot, json)?;
            Ok(true)
        }

        Commands::Autolink { min_similarity } => {
            let (new_links, nodes, edges) = client.autolink(Some(*min_similarity))?;
            CliView::render_autolink(new_links, nodes, edges, json)?;
            Ok(true)
        }

        Commands::Conflicts {
            yes,
            reranker,
            min_similarity,
        } => {
            let conflicts = client.conflicts(*yes, *min_similarity, reranker.as_deref())?;
            CliView::render_conflicts(&conflicts, *yes, Path::new(&client.base_url), json)?;
            Ok(true)
        }

        Commands::Observe {
            input,
            file,
            yes,
            dry_run,
            category,
            tags,
            confidence,
            reranker,
        } => {
            let raw_text = if let Some(ref path) = file {
                std::fs::read_to_string(path).with_context(|| {
                    format!("Failed to read conversation file: {}", path.display())
                })?
            } else if let Some(ref arg) = input {
                if arg == "-" {
                    let mut buf = String::new();
                    io::stdin().read_to_string(&mut buf)?;
                    buf
                } else if Path::new(arg).is_file() {
                    std::fs::read_to_string(arg)
                        .with_context(|| format!("Failed to read file: {}", arg))?
                } else {
                    arg.clone()
                }
            } else if !io::stdin().is_terminal() {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;
                buf
            } else {
                anyhow::bail!(
                    "Provide conversation text, a file path, or pipe via stdin: `lmem observe \"User: ...\"`"
                );
            };
            let should_save = *yes && !*dry_run;
            let req = lightmem::ObserveRequest {
                text: raw_text,
                category: *category,
                tags: tags
                    .as_ref()
                    .map(|t| parse_tag_list(std::slice::from_ref(t))),
                confidence: Some(*confidence),
                reranker: reranker.clone(),
                dry_run: !should_save,
            };
            let (candidates, saved) = client.observe(&req)?;
            if should_save {
                if json {
                    println!("{}", serde_json::to_string_pretty(&saved)?);
                } else {
                    CliView::render_observed_candidates(&candidates, false, false)?;
                    CliView::render_observed_summary(saved.len(), 0, false, false)?;
                }
            } else {
                CliView::render_observed_candidates(&candidates, true, json)?;
            }
            Ok(true)
        }

        Commands::Context => {
            let stats_res = client.stats();
            let health_res = client.health();
            let is_online = health_res.is_ok();
            let info = serde_json::json!({
                "mode": "remote",
                "remote_url": client.base_url,
                "status": if is_online { "online" } else { "unreachable" },
                "api_key_configured": client.api_key.is_some(),
                "total_memories": stats_res.as_ref().map(|s| s.total_memories).unwrap_or(0),
                "active_memories": stats_res.as_ref().map(|s| s.active_memories).unwrap_or(0),
            });
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!();
                println!(
                    "  {} {}",
                    "❖".red().bold(),
                    "LightMem Remote Context".white().bold()
                );
                println!(
                    "  {}",
                    "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━".red()
                );
                println!(
                    "  {} {:<20} {}",
                    "├─".dimmed(),
                    "Remote Endpoint",
                    client.base_url.blue()
                );
                println!(
                    "  {} {:<20} {}",
                    "├─".dimmed(),
                    "Server Status",
                    if is_online {
                        "● Online".green().bold()
                    } else {
                        "○ Unreachable".red().bold()
                    }
                );
                println!(
                    "  {} {:<20} {}",
                    "├─".dimmed(),
                    "Authentication",
                    if client.api_key.is_some() {
                        "Bearer Token (Configured)".yellow()
                    } else {
                        "None".dimmed()
                    }
                );
                if let Ok(st) = stats_res {
                    println!(
                        "  {} {:<20} {} active ({} total)",
                        "└─".dimmed(),
                        "Remote Vault",
                        st.active_memories.to_string().bold(),
                        st.total_memories
                    );
                } else {
                    println!(
                        "  {} {:<20} {}",
                        "└─".dimmed(),
                        "Remote Vault",
                        "Could not fetch remote stats".red()
                    );
                }
                println!();
            }
            Ok(true)
        }

        Commands::Config { .. }
        | Commands::Model { .. }
        | Commands::Setup { .. }
        | Commands::Docs { .. }
        | Commands::Bundle {
            action: BundleCommand::Info { .. },
            ..
        }
        | Commands::Serve { .. }
        | Commands::Version
        | Commands::Completions { .. }
        | Commands::Connect { mcp: false, .. }
        | Commands::Uninstall { .. }
        | Commands::History { .. }
        | Commands::Local
        | Commands::Remote { .. } => Ok(false),
        cmd => {
            let cmd_name = match cmd {
                Commands::Bundle {
                    action: BundleCommand::Export { .. },
                    ..
                } => "bundle export",
                Commands::Bundle {
                    action: BundleCommand::Import { .. },
                    ..
                } => "bundle import",
                Commands::Restore { .. } => "restore",
                Commands::Import { .. } => "import",
                Commands::Export { .. } => "export",
                Commands::Reindex { .. } => "reindex",
                Commands::Connect { mcp: true, .. } => "connect --mcp",
                #[cfg(feature = "mcp")]
                Commands::Mcp => "mcp",
                _ => "this command",
            };
            let reason_msg = format!(
                "This command ('{cmd_name}') is not supported in remote mode. No local vault was opened."
            );
            if json {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "status": "error",
                        "error": "unsupported_remote_command",
                        "command": cmd_name,
                        "remote_url": client.base_url,
                        "message": reason_msg,
                        "hint": "Use --db PATH to explicitly select a local vault, or switch to local mode with 'lmem local'."
                    })
                );
            } else {
                CliView::render_error(
                    &CliErrorCard {
                        title: "Command Not Supported in Remote Mode",
                        reason: &reason_msg,
                        details: &[
                            ("Command", cmd_name),
                            ("Active Mode", &format!("Remote ({})", client.base_url)),
                        ],
                        suggestions: &[
                            ("Switch to local:", "lmem local"),
                            (
                                "Explicit local vault:",
                                &format!("lmem --db ~/.lightmem/memories.db {cmd_name}"),
                            ),
                        ],
                    },
                    false,
                );
            }
            Err(AlreadyHandledError::new(reason_msg).into())
        }
    }
}
