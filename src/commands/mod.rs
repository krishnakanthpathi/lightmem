#![allow(clippy::too_many_arguments)]

pub mod answer;
pub mod config;
pub mod context;
pub mod graph;
pub mod manage;
pub mod observe;
pub mod recall;
pub mod remember;
pub mod remote;
pub mod service;
pub mod transfer;

use anyhow::Result;
use lightmem::{CliView, LightMemConfig};
use std::path::{Path, PathBuf};

use crate::terminal;
use crate::{Cli, Commands};
pub use context::AlreadyHandledError;

pub fn run(cli: Cli) -> Result<()> {
    if cli.version {
        return CliView::render_version(cli.json);
    }
    if let Some(Commands::Completions { shell }) = &cli.command {
        return service::print_completions(*shell);
    }
    if matches!(cli.command, Some(Commands::CompletionConnections)) {
        for id in terminal::connection_ids()? {
            println!("{id}");
        }
        return Ok(());
    }
    if let Some(Commands::Doctor) = &cli.command {
        return config::handle_doctor(
            &cli.db,
            cli.global,
            cli.connection.as_deref(),
            cli.scope.clone(),
            cli.session.as_deref(),
            cli.remote.as_deref(),
            cli.json,
        );
    }
    if let Some(Commands::Local) = &cli.command {
        return config::handle_local(cli.global, cli.json);
    }
    if let Some(Commands::Remote { url, api_key }) = &cli.command {
        return config::handle_remote(url.clone(), api_key.clone(), cli.json);
    }

    let env_db = std::env::var("LIGHTMEM_DB")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from);
    let default_db = cli
        .db
        .as_deref()
        .map(Path::to_path_buf)
        .or_else(|| env_db.clone())
        .unwrap_or_else(|| LightMemConfig::resolve_db_path(cli.global));

    if let Some(Commands::Identity { action }) = &cli.command {
        return service::handle_identity(action, cli.json, default_db);
    }
    if let Some(Commands::Status) = &cli.command {
        return manage::handle_status(cli.json, default_db);
    }

    let connection_id = cli.connection.clone().or_else(|| {
        std::env::var("LIGHTMEM_CONNECTION")
            .ok()
            .filter(|v| !v.trim().is_empty())
    });
    anyhow::ensure!(
        connection_id.is_some() || (cli.scope.is_none() && cli.session.is_none()),
        "--scope and --session require --connection"
    );
    anyhow::ensure!(
        connection_id.is_none() || (cli.db.is_none() && env_db.is_none() && !cli.global),
        "A connection cannot be combined with --db, LIGHTMEM_DB, or --global"
    );
    let resolved_connection = connection_id
        .as_deref()
        .map(|id| {
            lightmem::IdentityRegistry::open_default()?.resolve(
                id,
                cli.scope.clone(),
                cli.session.clone(),
            )
        })
        .transpose()?;
    let connection = resolved_connection.as_ref();
    let effective_db = cli
        .db
        .as_deref()
        .or(env_db.as_deref())
        .or_else(|| connection.map(|c| c.db_path.as_path()));
    let global = cli.global;

    let cfg = if matches!(cli.command, Some(Commands::Version)) {
        LightMemConfig::default()
    } else {
        LightMemConfig::try_load()?
    };
    anyhow::ensure!(
        cli.remote.is_none() || (cli.db.is_none() && env_db.is_none() && !cli.global),
        "--remote cannot be combined with --db, LIGHTMEM_DB, or --global"
    );
    let effective_remote_url = if cli.remote.is_some() {
        cli.remote
    } else if cli.db.is_some() || env_db.is_some() || cli.global {
        None
    } else {
        cfg.effective_remote_url()
    };
    let effective_api_key = cli.api_key.or_else(|| cfg.effective_api_key());

    if let Some(ref remote_url) = effective_remote_url {
        let local_admin = matches!(
            cli.command,
            Some(
                Commands::Config { .. }
                    | Commands::Model { .. }
                    | Commands::Setup { .. }
                    | Commands::Docs { .. }
                    | Commands::Serve { .. }
                    | Commands::Version
                    | Commands::Completions { .. }
                    | Commands::Connect { mcp: false, .. }
                    | Commands::Uninstall { .. }
                    | Commands::History { .. }
            )
        );
        anyhow::ensure!(
            !LightMemConfig::offline_requested() || local_admin,
            "Offline mode blocks remote vault operations. Select a local database or clear remote routing with 'lmem local' (or 'lmem config --clear-remote')."
        );
        anyhow::ensure!(
            connection.is_none() || local_admin,
            "Remote mode does not support local connections or scopes. Clear remote mode with 'lmem local' (or 'lmem config --clear-remote') before selecting a connection."
        );
        let client = lightmem::RemoteClient::new(remote_url, effective_api_key.clone());
        if cli.command.is_none() {
            let stats = match client.stats() {
                Ok(s) => s,
                Err(ref e) => context::handle_remote_network_error(remote_url, e, cli.json),
            };
            if cli.json {
                return CliView::render_stats(&stats, Path::new(remote_url), true);
            }
            CliView::render_intro(Path::new(remote_url), &stats);
            return Ok(());
        }
        if let Some(ref cmd) = cli.command {
            match remote::handle_remote_command(&client, cmd, cli.json) {
                Ok(true) => return Ok(()),
                Ok(false) => {}
                Err(e) => {
                    let err_str = format!("{e:#}");
                    let is_network_or_auth = err_str.contains("401")
                        || err_str.contains("Unauthorized")
                        || err_str.contains("Authentication failed")
                        || err_str.contains("Failed to reach remote server");
                    if is_network_or_auth && !err_str.contains("not supported in remote mode") {
                        context::handle_remote_network_error(remote_url, &e, cli.json);
                    }
                    return Err(e);
                }
            }
        }
    }

    let Some(command) = cli.command else {
        let lm = context::open_controller(effective_db, global, connection)?;
        let stats = lm.stats()?;
        if cli.json {
            return CliView::render_stats(&stats, lm.db_path(), true);
        }
        CliView::render_intro(lm.db_path(), &stats);
        return Ok(());
    };

    match command {
        Commands::Identity { .. }
        | Commands::Status
        | Commands::Doctor
        | Commands::CompletionConnections
        | Commands::Local
        | Commands::Remote { .. } => unreachable!(),

        Commands::Context => manage::handle_context(connection, effective_db, global, cli.json),

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
        } => remember::handle_remember(
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
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Recall {
            query,
            image,
            category,
            tag,
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
        } => recall::handle_recall(
            query,
            image,
            category,
            tag,
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
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Answer {
            question,
            precision,
            reranker,
            category,
            as_of,
            date,
            limit,
        } => answer::handle_answer(
            question,
            precision,
            reranker,
            category,
            as_of,
            date,
            limit,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::List {
            category,
            tag,
            media,
            status,
            as_of,
            date,
            limit,
            offset,
            page,
            save_to,
            path,
        } => manage::handle_list(
            category,
            tag,
            media,
            status,
            as_of,
            date,
            limit,
            offset,
            page,
            save_to,
            path,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Inspect {
            terms,
            save_to,
            path,
            open,
        } => manage::handle_inspect(
            terms,
            save_to,
            path,
            open,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Update {
            id,
            content,
            title,
            category,
            tags,
            confidence,
        } => manage::handle_update(
            id,
            content,
            title,
            category,
            tags,
            confidence,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Deduplicate => manage::handle_dedup(cli.json, effective_db, global, connection),

        Commands::Forget { id, all, hard, yes } => manage::handle_forget(
            id,
            all,
            hard,
            yes,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Restore { term } => {
            manage::handle_restore(term, cli.json, effective_db, global, connection)
        }

        Commands::Clear { soft, yes } => {
            manage::handle_clear(soft, yes, cli.json, effective_db, global, connection)
        }

        Commands::Export {
            format,
            okf,
            bundle,
            output,
            output_json,
        } => transfer::handle_export(
            format,
            okf,
            bundle,
            output,
            output_json,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Import {
            file,
            enrich,
            replace,
            merge,
        } => transfer::handle_import(
            file,
            enrich,
            replace,
            merge,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Bundle { action } => {
            transfer::handle_bundle(action, cli.json, effective_db, global, connection)
        }

        Commands::Conflicts {
            yes,
            reranker,
            min_similarity,
        } => graph::handle_conflicts(
            yes,
            reranker,
            min_similarity,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Link {
            source,
            target,
            relation,
            weight,
        } => graph::handle_link(
            source,
            target,
            relation,
            weight,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Unlink {
            source,
            target,
            relation,
        } => graph::handle_unlink(
            source,
            target,
            relation,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Links { id } => {
            graph::handle_links(id, cli.json, effective_db, global, connection)
        }

        Commands::Tags { query } => {
            graph::handle_tags(query, cli.json, effective_db, global, connection)
        }

        Commands::Related { id, hops } => {
            graph::handle_related(id, hops, cli.json, effective_db, global, connection)
        }

        Commands::Graph { focus, hops } => {
            graph::handle_graph(focus, hops, cli.json, effective_db, global, connection)
        }

        Commands::Autolink { min_similarity } => {
            graph::handle_autolink(min_similarity, cli.json, effective_db, global, connection)
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
        } => observe::handle_observe(
            input,
            file,
            yes,
            dry_run,
            category,
            tags,
            confidence,
            reranker,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Model {
            provider_or_model,
            api_key,
            endpoint,
        } => config::handle_model(
            provider_or_model,
            api_key,
            endpoint,
            cli.json,
            effective_db,
            global,
        ),

        Commands::Config {
            reload,
            action,
            edit,
            preset,
            backend,
            onnx_model,
            embedding_endpoint,
            embedding_api_key,
            download,
            url,
            model,
            reranker,
            entity_extraction,
            ollama_api_key,
            dry_run,
            test,
            reset_db,
            yes,
            remote,
            api_key,
            clear_remote,
        } => config::handle_config(
            reload,
            action,
            edit,
            preset,
            backend,
            onnx_model,
            embedding_endpoint,
            embedding_api_key,
            download,
            url,
            model,
            reranker,
            entity_extraction,
            ollama_api_key,
            dry_run,
            test,
            reset_db,
            yes,
            remote,
            api_key,
            clear_remote,
            cli.json,
            effective_db,
            global,
            connection,
        ),

        Commands::Setup {
            preset,
            url,
            model,
            api_key,
            entity_extraction,
            dry_run,
            yes,
        } => config::handle_setup(
            preset,
            url,
            model,
            api_key,
            entity_extraction,
            dry_run,
            yes,
            cli.json,
            effective_db,
            global,
        ),

        Commands::Reindex { yes } => {
            config::handle_reindex(yes, cli.json, effective_db, global, connection)
        }

        Commands::Stats => manage::handle_stats(cli.json, effective_db, global, connection),

        Commands::Connect {
            platform,
            workspace,
            path,
            list,
            mcp,
        } => service::handle_connect(platform, workspace, path, list, cli.json, mcp, connection),

        #[cfg(feature = "mcp")]
        Commands::Mcp => service::handle_mcp(global, effective_db, connection),

        Commands::Completions { shell } => service::print_completions(shell),

        Commands::Version => CliView::render_version(cli.json),

        Commands::Docs {
            port,
            landing,
            no_browser,
        } => service::handle_docs(port, landing, no_browser),

        Commands::Serve {
            host,
            port,
            api_key,
            no_auth,
            daemon,
            stop,
            status,
        } => service::handle_serve(
            host,
            port,
            api_key,
            no_auth,
            daemon,
            stop,
            status,
            effective_db,
            global,
            connection,
            cli.db.as_deref(),
            cli.connection.as_deref(),
        ),

        Commands::Uninstall { purge_data, yes } => {
            service::handle_uninstall(purge_data, yes, cli.json)
        }

        Commands::History {
            query,
            limit,
            status,
            clear,
        } => manage::handle_history(query, limit, status, clear, cli.json),
    }
}
