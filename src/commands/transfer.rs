use anyhow::Result;
use lightmem::{CliView, ResolvedConnection};
use std::io::Write;
use std::path::{Path, PathBuf};

use super::config::migrate_embeddings;
use super::context::open_controller;
use crate::terminal;
use crate::BundleCommand;

pub fn handle_export(
    format: String,
    okf: bool,
    bundle: bool,
    output: Option<PathBuf>,
    output_json: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    let lm = open_controller(effective_db, global, connection)?;
    let is_bundle = bundle
        || format == "bundle"
        || output
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("zip") || e.eq_ignore_ascii_case("lmbundle"))
            .unwrap_or(false);

    let is_okf = okf || format == "okf";

    if is_bundle {
        let exported_path = lm.export_bundle(output.as_deref())?;
        let manifest = lm.inspect_bundle(&exported_path)?;
        CliView::render_bundle_export(&exported_path, &manifest, output_json || json)?;
    } else if is_okf {
        let exported_path = lm.export_okf(output.as_deref())?;
        CliView::render_export(&exported_path, false, output_json || json)?;
    } else {
        let target = output.unwrap_or_else(|| PathBuf::from("lightmem-backup.json"));
        let exported_path = lm.export_json(&target)?;
        CliView::render_export(&exported_path, true, output_json || json)?;
    }
    Ok(())
}

pub fn handle_import(
    file: PathBuf,
    enrich: bool,
    replace: bool,
    merge: bool,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    anyhow::ensure!(
        !enrich,
        "Import enrichment is not supported yet. Remove --enrich; use observe --dry-run to review extracted candidates."
    );
    let lm = open_controller(effective_db, global, connection)?;
    let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("zip") || ext.eq_ignore_ascii_case("lmbundle") {
        let res = lm.import_bundle(&file, replace, merge)?;
        CliView::render_bundle_import(&res, json)?;
    } else {
        if !migrate_embeddings(&lm, false, json, false)? {
            return Ok(());
        }
        let count = lm.import_file_with_options(&file, enrich, replace)?;
        CliView::render_import(count, &file, json)?;
    }
    Ok(())
}

pub fn handle_bundle(
    action: BundleCommand,
    json: bool,
    effective_db: Option<&Path>,
    global: bool,
    connection: Option<&ResolvedConnection>,
) -> Result<()> {
    match action {
        BundleCommand::Export {
            output,
            json: cmd_json,
        } => {
            let is_json = json || cmd_json;
            let lm = open_controller(effective_db, global, connection)?;
            let mut progress_bar = terminal::TerminalProgress::new("Packaging", 0);
            if !is_json {
                eprintln!();
                eprintln!(
                    "  {} {}",
                    CliView::crimson_bold("❖"),
                    CliView::white_bold("L I G H T M E M  Bundle Export")
                );
                eprintln!(
                    "  {}",
                    CliView::crimson(
                        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    )
                );
                if let Some(out_p) = output.as_deref() {
                    eprintln!(
                        "  {} {:<16} {}",
                        CliView::slate("◫"),
                        CliView::white_bold("Destination:"),
                        out_p.display()
                    );
                }
                eprintln!(
                    "  {} {:<16} {}",
                    CliView::slate("◈"),
                    CliView::white_bold("Vault:"),
                    lm.db_path().display()
                );
                eprintln!();
            }

            let exported_path = lm.export_bundle_with_progress(output.as_deref(), |prog| {
                if is_json {
                    return;
                }
                match prog {
                    lightmem::services::BundleProgress::StageStart {
                        stage,
                        total_stages,
                        title,
                    } => {
                        if stage == 1 || stage == 2 {
                            eprint!(
                                "  {} [{}/{}] {:<38}",
                                CliView::slate("▸"),
                                stage,
                                total_stages,
                                title
                            );
                            let _ = std::io::stderr().flush();
                        }
                    }
                    lightmem::services::BundleProgress::StageDone { stage, summary } => {
                        if stage == 1 || stage == 2 {
                            eprintln!(" {} {}", CliView::emerald_bold("✓"), summary);
                        } else if stage == 3 {
                            progress_bar.finish("Archive finalized");
                        }
                    }
                    lightmem::services::BundleProgress::FileProgress {
                        current,
                        total,
                        filename,
                        ..
                    } => {
                        progress_bar.set_total(total);
                        progress_bar.update(current, &filename);
                    }
                }
            })?;

            let manifest = lm.inspect_bundle(&exported_path)?;
            if !is_json {
                eprintln!();
            }
            CliView::render_bundle_export(&exported_path, &manifest, is_json)?;
        }
        BundleCommand::Import {
            file,
            replace,
            merge,
            json: cmd_json,
        } => {
            let is_json = json || cmd_json;
            let lm = open_controller(effective_db, global, connection)?;
            let mut progress_bar = terminal::TerminalProgress::new("Extracting", 0);
            if !is_json {
                eprintln!();
                eprintln!(
                    "  {} {}",
                    CliView::crimson_bold("❖"),
                    CliView::white_bold("L I G H T M E M  Bundle Import")
                );
                eprintln!(
                    "  {}",
                    CliView::crimson(
                        "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
                    )
                );
                eprintln!(
                    "  {} {:<16} {}",
                    CliView::slate("◫"),
                    CliView::white_bold("Bundle Archive:"),
                    file.display()
                );
                eprintln!(
                    "  {} {:<16} {}",
                    CliView::slate("◈"),
                    CliView::white_bold("Mode:"),
                    if replace {
                        "Overwrite / Replace active vault (--replace)"
                    } else if merge {
                        "Merge into active vault (--merge)"
                    } else {
                        "Import into empty vault"
                    }
                );
                eprintln!();
            }

            let res = lm.import_bundle_with_progress(&file, replace, merge, |prog| {
                if is_json {
                    return;
                }
                match prog {
                    lightmem::services::BundleProgress::StageStart {
                        stage,
                        total_stages,
                        title,
                    } => {
                        if stage == 1 || stage == 3 {
                            eprint!(
                                "  {} [{}/{}] {:<42}",
                                CliView::slate("▸"),
                                stage,
                                total_stages,
                                title
                            );
                            let _ = std::io::stderr().flush();
                        }
                    }
                    lightmem::services::BundleProgress::StageDone { stage, summary } => {
                        if stage == 1 || stage == 3 {
                            eprintln!(" {} {}", CliView::emerald_bold("✓"), summary);
                        } else if stage == 2 {
                            progress_bar.finish("All attachments extracted");
                        }
                    }
                    lightmem::services::BundleProgress::FileProgress {
                        current,
                        total,
                        filename,
                        ..
                    } => {
                        progress_bar.set_total(total);
                        progress_bar.update(current, &filename);
                    }
                }
            })?;

            if !is_json {
                eprintln!();
            }
            CliView::render_bundle_import(&res, is_json)?;
        }
        BundleCommand::Info {
            file,
            json: cmd_json,
        } => {
            let manifest = lightmem::services::bundle::BundleService::inspect_bundle(&file)?;
            CliView::render_bundle_info(&manifest, &file, json || cmd_json)?;
        }
    }
    Ok(())
}
