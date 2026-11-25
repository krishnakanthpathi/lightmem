use anyhow::{Context, Result};
use colored::*;
use lightmem::{CliErrorCard, CliView, RemoteClient};
use std::io::{IsTerminal, Write};
use std::path::Path;

use super::context::AlreadyHandledError;
use crate::{BundleCommand, Commands};

pub fn handle_remote_command(
    client: &RemoteClient,
    command: &Commands,
    json: bool,
) -> Result<bool> {
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
            let mut attachment_payloads = Vec::new();
            for fpath in files {
                let data = std::fs::read(fpath).with_context(|| {
                    format!("Failed to read attachment file: {}", fpath.display())
                })?;
                let fname = fpath
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                let (mime, _) = lightmem::detect_mime_and_kind(&fname, &data);
                let data_base64 = lightmem::encode_base64(&data);
                attachment_payloads.push(lightmem::AttachmentPayload {
                    name: fname,
                    mime: Some(mime),
                    data_base64,
                });
            }
            let req = lightmem::RememberRequest {
                content: content.clone(),
                category: *category,
                tags: tags
                    .as_ref()
                    .map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
                source: provenance.clone(),
                title: title.clone(),
                ttl: ttl.map(|d| format!("{}ms", d.num_milliseconds())),
                confidence: Some(*confidence),
                supersede: *supersede,
                no_entities: Some(*no_entities),
                attachments: if attachment_payloads.is_empty() {
                    None
                } else {
                    Some(attachment_payloads)
                },
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
            tag: _,
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
            if *precision {
                anyhow::ensure!(
                    media.is_none() && !multi_hop && min_similarity.is_none(),
                    "Precision recall returns an answer; media, multi-hop, and similarity filters are not supported. Use recall without --precision for filtered results."
                );
                let answer = match client.answer(&lightmem::AnswerRequest {
                    question: query.clone(),
                    category: *category,
                    as_of: *as_of,
                    date: *date,
                    limit: *limit,
                    ..Default::default()
                }) {
                    Ok(ans) => ans,
                    Err(err) => {
                        CliView::render_llm_or_error(&err, json);
                        return Err(AlreadyHandledError.into());
                    }
                };
                CliView::render_answer(&answer, json)?;
                return Ok(true);
            }
            let req = lightmem::RecallRequest {
                query: query.clone(),
                category: *category,
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
            as_of,
            date,
            limit,
        } => {
            let req = lightmem::AnswerRequest {
                question: question.clone(),
                precision: *precision,
                reranker: reranker.clone(),
                category: *category,
                as_of: *as_of,
                date: *date,
                limit: *limit,
            };
            let result = match client.answer(&req) {
                Ok(ans) => ans,
                Err(err) => {
                    CliView::render_llm_or_error(&err, json);
                    return Err(AlreadyHandledError.into());
                }
            };
            CliView::render_answer(&result, json)?;
            Ok(true)
        }

        Commands::List {
            category,
            tag: _,
            media,
            status,
            as_of,
            date,
            limit,
            offset,
            page,
            save_to,
            path,
        } => {
            anyhow::ensure!(
                save_to.is_none() && !path,
                "Remote list does not support --save-to or --path. Use local mode to download attachments."
            );
            let paginated = client.list(
                *limit,
                *offset,
                *page,
                *category,
                Some(*status),
                media.clone(),
                *date,
                *as_of,
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

        Commands::Forget { id, all, hard, yes } => {
            if *all && !*yes && !json {
                if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
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
            let ok = client.forget(id.as_deref(), *all, *hard)?;
            CliView::render_forget(id.as_deref().unwrap_or("*"), ok, *hard, json)?;
            Ok(true)
        }

        Commands::Clear { soft, yes } => {
            if !*yes && !json {
                if std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
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
                Commands::Clear { .. } => "clear",
                Commands::Restore { .. } => "restore",
                Commands::Inspect { .. } => "inspect",
                Commands::Import { .. } => "import",
                Commands::Export { .. } => "export",
                Commands::Conflicts { .. } => "conflicts",
                Commands::Autolink { .. } => "autolink",
                Commands::Connect { mcp: true, .. } => "connect --mcp",
                _ => "this command",
            };
            let reason_msg = format!("This command ('{cmd_name}') is not supported in remote mode. No local vault was opened.");
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
            std::process::exit(1);
        }
    }
}
