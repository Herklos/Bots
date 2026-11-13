//! The `hbots anytype` commands: Prepare the space, and survey an HK org
//! space read-only before an import (docs/anytype-hk-sync-plan.md).
//!
//!   hbots anytype prepare [--dry-run]
//!   hbots anytype hk-survey --hk-space <name-or-id> [--json]

use crate::platform::config::Config;
use serde_json::Value;
use super::env::Flags;

pub(crate) async fn run(flags: &Flags, rest: &[String]) -> anyhow::Result<()> {
    let sub = rest.first().map(String::as_str).unwrap_or("");
    match sub {
        "prepare" => prepare(flags, !flags.has("dry-run")).await,
        "hk-survey" => hk_survey(flags, flags.has("json")).await,
        "hk-import" => hk_import(flags, flags.has("apply")).await,
        other => {
            if other.is_empty() {
                println!("Usage: hbots anytype <prepare|hk-survey> [options]");
                Ok(())
            } else {
                anyhow::bail!("unknown `hbots anytype` command: {other}")
            }
        }
    }
}

async fn open_bots_client(
    cfg: &Config,
) -> anyhow::Result<crate::storage::adapters::anytype::AnytypeClient> {
    let stored = crate::storage::settings::read_load(&cfg.data_root).stored;
    let host = crate::storage::settings::effective_host(&cfg.storage, &stored);
    let space = crate::storage::settings::effective_space(&cfg.storage, &stored);
    let key = crate::storage::settings::read_key(&cfg.data_root, &cfg.storage)
        .ok_or_else(|| anyhow::anyhow!("no Anytype key in {}", cfg.data_root.display()))?;
    crate::storage::adapters::anytype::open_client(&host, &key, &space)
        .await
        .map_err(|error| anyhow::anyhow!("{}", error.message))
}

fn error_note(error: &crate::storage::adapters::anytype::AnytypeError) -> String {
    crate::storage::adapters::anytype::typed::error_note(error)
}

async fn prepare(_flags: &Flags, write: bool) -> anyhow::Result<()> {
    let cfg = Config::from_env()?;
    let client = open_bots_client(&cfg).await?;
    let report = crate::storage::adapters::anytype::bootstrap(&client, !write)
        .await
        .map_err(|error| anyhow::anyhow!("{}", error_note(&error)))?;
    let mode = if report.dry_run { "dry run" } else { "prepared" };
    println!(
        "{mode}: {} types, {} properties, meta {}",
        report.types.len(),
        report.properties.len(),
        if report.meta { "write" } else { "ok" }
    );
    Ok(())
}

async fn hk_survey(flags: &Flags, as_json: bool) -> anyhow::Result<()> {
    let hk_space = flags
        .value("hk-space")
        .filter(|space| !space.is_empty() && *space != "true")
        .ok_or_else(|| anyhow::anyhow!("hk-survey needs --hk-space <name-or-id>"))?
        .to_string();
    let cfg = Config::from_env()?;
    let (bots, hk) = survey_clients(&cfg, &hk_space).await?;
    let report = crate::storage::adapters::anytype::typed::hk::survey(&bots, &hk).await;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_survey(&report);
    }
    Ok(())
}

async fn survey_clients(
    cfg: &Config,
    hk_space: &str,
) -> anyhow::Result<(
    crate::storage::adapters::anytype::AnytypeClient,
    crate::storage::adapters::anytype::AnytypeClient,
)> {
    let stored = crate::storage::settings::read_load(&cfg.data_root).stored;
    let host = crate::storage::settings::effective_host(&cfg.storage, &stored);
    let bots_space = crate::storage::settings::effective_space(&cfg.storage, &stored);
    let key = crate::storage::settings::read_key(&cfg.data_root, &cfg.storage)
        .ok_or_else(|| anyhow::anyhow!("no Anytype key in {}", cfg.data_root.display()))?;
    let bots = crate::storage::adapters::anytype::open_client(&host, &key, &bots_space)
        .await
        .map_err(|error| anyhow::anyhow!("Bots space: {}", error.message))?;
    let hk = crate::storage::adapters::anytype::open_client(&host, &key, hk_space)
        .await
        .map_err(|error| anyhow::anyhow!("HK space: {}", error.message))?;
    Ok((bots, hk))
}

async fn hk_import(flags: &Flags, apply: bool) -> anyhow::Result<()> {
    let hk_space = flags
        .value("hk-space")
        .filter(|space| !space.is_empty() && *space != "true")
        .ok_or_else(|| anyhow::anyhow!("hk-import needs --hk-space <name-or-id>"))?
        .to_string();
    let cfg = Config::from_env()?;
    let (bots, hk) = survey_clients(&cfg, &hk_space).await?;
    let report = crate::storage::adapters::anytype::typed::hk_import::run(&bots, &hk, apply).await;
    let mode = if apply { "applied" } else { "dry run" };
    println!(
        "{}: {} would-create, {} created, {} updated, {} unresolved links, {} raw fields, {} errors",
        mode,
        report["staged"].as_i64().unwrap_or(0),
        report["created"].as_array().map(Vec::len).unwrap_or(0),
        report["updated"].as_array().map(Vec::len).unwrap_or(0),
        report["unresolved"].as_array().map(Vec::len).unwrap_or(0),
        report["raw_fields"].as_i64().unwrap_or(0),
        report["errors"].as_array().map(Vec::len).unwrap_or(0),
    );
    for item in report["errors"].as_array().into_iter().flatten() {
        println!("error: {}", item.as_str().unwrap_or(""));
    }
    if !apply && report["staged"].as_i64().unwrap_or(0) > 0 {
        println!("re-run with --apply to write");
    }
    Ok(())
}

fn print_survey(report: &Value) {
    println!("HK org space survey (read-only)");
    println!("{:<18} {:>6}  destination", "type", "count");
    for row in report["types"].as_array().into_iter().flatten() {
        println!(
            "{:<18} {:>6}  {}",
            row["hk"].as_str().unwrap_or(""),
            row["count"].as_i64().unwrap_or(0),
            row["bots"].as_str().unwrap_or("preserved raw")
        );
    }
    println!();
    println!(
        "sample agent: {} fields",
        report["agent_sample"]["fields"].as_i64().unwrap_or(0)
    );
    for (label, note) in [
        ("typed", "typed properties"),
        ("folded", "folded into JSON contracts"),
        ("raw", "kept in m_source_json"),
        ("local", "local only"),
        ("unmapped", "unmapped (rule gap)"),
    ] {
        let list: Vec<&str> = report["agent_sample"][label]
            .as_array()
            .map(|rows| rows.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !list.is_empty() {
            println!("  {:>15} ({:>2})  {}", note, list.len(), list.join(", "));
        }
    }
}
