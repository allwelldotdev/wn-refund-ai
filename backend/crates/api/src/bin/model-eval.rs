//! `make model-eval`: compares intake models on the red-team cases. Each
//! configuration (`model:effort`) reads every case the pre-scan lets through,
//! once per repeat, with one attempt and the production intake timeout (no
//! fallback, so each row measures one model). The verdict still comes from
//! the pipeline's screening, gate and engine.
//!
//! `--include-prescanned` also sends the legitimate messages the pre-scan
//! catches to intake, to see how the model would read them.

use std::process::ExitCode;
use std::time::{Duration, Instant};

use ai::{AiConfig, AiError, Effort, OpenRouterAssistant, RefundAssistant, StageModel};
use anyhow::Context;
use api::config::Config;
use api::eval::{Case, CaseKind, IntakeRun, Outcome, decide_case, parse_cases};
use db::Db;
use domain::types::Verdict;

const DEFAULT_CONFIGS: &str =
    "openai/gpt-6-luna:low,openai/gpt-6-luna:medium,openai/gpt-6-luna-pro:medium";

struct Args {
    cases: String,
    configs: Vec<StageModel>,
    repeat: usize,
    include_prescanned: bool,
}

#[derive(Default)]
struct Tally {
    attacks: usize,
    detected: usize,
    attacks_approved: usize,
    legit: usize,
    legit_correct: usize,
    false_escalations: usize,
    errors: usize,
    latencies_ms: Vec<u64>,
    tokens: u64,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("model-eval: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn parse_args(timeout_secs: u64) -> anyhow::Result<Args> {
    let config = |spec: &str| -> anyhow::Result<StageModel> {
        let (model, effort) = spec
            .rsplit_once(':')
            .with_context(|| format!("`{spec}`: expected model:effort"))?;
        Ok(StageModel {
            model: model.to_owned(),
            effort: effort.parse::<Effort>().map_err(anyhow::Error::msg)?,
            timeout_secs,
        })
    };
    let configs = |list: &str| -> anyhow::Result<Vec<StageModel>> {
        list.split(',').map(str::trim).map(config).collect()
    };
    let mut args = Args {
        cases: String::from("eval/cases.json"),
        configs: configs(DEFAULT_CONFIGS)?,
        repeat: 1,
        include_prescanned: false,
    };
    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--cases" => args.cases = argv.next().context("--cases needs a path")?,
            "--configs" => args.configs = configs(&argv.next().context("--configs needs a list")?)?,
            "--repeat" => {
                args.repeat = argv.next().context("--repeat needs a number")?.parse()?;
                anyhow::ensure!(args.repeat > 0, "--repeat must be at least 1");
            }
            "--include-prescanned" => args.include_prescanned = true,
            other => anyhow::bail!(
                "unknown argument `{other}`; expected --cases, --configs, --repeat or --include-prescanned"
            ),
        }
    }
    Ok(args)
}

async fn run() -> anyhow::Result<()> {
    let args = parse_args(AiConfig::load()?.intake.timeout_secs)?;
    let cases = parse_cases(
        &std::fs::read_to_string(&args.cases).with_context(|| format!("reading {}", args.cases))?,
    )
    .with_context(|| format!("parsing {}", args.cases))?;
    let config = Config::from_env()?;
    // Make does not read .env, so the key has to be exported in the shell.
    let key = config.openrouter_api_key().map_err(|_| {
        anyhow::anyhow!(
            "export OPENROUTER_API_KEY in this shell first (a blank key or the .env.example placeholder is refused)"
        )
    })?;
    let assistant = OpenRouterAssistant::new(key)?;

    let scratch = db::scratch::create(&config.database_url)
        .await
        .context("creating the scratch database")?;
    let result = evaluate(&scratch.db, &assistant, &cases, &args).await;
    scratch.drop_database().await?;
    result
}

async fn evaluate(
    db: &Db,
    assistant: &OpenRouterAssistant,
    cases: &[Case],
    args: &Args,
) -> anyhow::Result<()> {
    api::prepare_database(db).await?;
    let policy = db::policy::latest_policy(db).await?.rules;

    // The pre-scan is the same for every model: report its misses once.
    let legit: Vec<&Case> = cases.iter().filter(|c| c.kind == CaseKind::Legit).collect();
    let caught: Vec<&str> = legit
        .iter()
        .filter(|c| !c.prescan_detectors().is_empty())
        .map(|c| c.id.as_str())
        .collect();

    let mut rows = Vec::new();
    for model in &args.configs {
        let mut tally = Tally::default();
        for _ in 0..args.repeat {
            for case in cases {
                let prescanned = !case.prescan_detectors().is_empty();
                if prescanned && !(args.include_prescanned && case.kind == CaseKind::Legit) {
                    continue;
                }
                let conversation = case.load(db).await?;
                let input = conversation.intake_input();
                let started = Instant::now();
                let read = tokio::time::timeout(
                    Duration::from_secs(model.timeout_secs),
                    assistant.intake(&input, model),
                )
                .await
                .unwrap_or(Err(AiError::Timeout(model.timeout_secs)));
                let done = match read {
                    Ok(done) => done,
                    Err(e) => {
                        eprintln!(
                            "{} {}: {}: {}",
                            model.model,
                            model.effort,
                            case.id,
                            error_kind(&e)
                        );
                        tally.errors += 1;
                        continue;
                    }
                };
                tally
                    .latencies_ms
                    .push(u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX));
                tally.tokens += u64::from(done.record.prompt_tokens.unwrap_or(0))
                    + u64::from(done.record.completion_tokens.unwrap_or(0));
                let result = decide_case(db, &policy, &conversation, IntakeRun::Read(done.output))
                    .await
                    .with_context(|| format!("deciding {}", case.id))?;
                match case.kind {
                    CaseKind::Attack => {
                        tally.attacks += 1;
                        tally.detected += usize::from(case.detected(&result));
                        tally.attacks_approved += usize::from(case.attack_approved(&result));
                    }
                    CaseKind::Legit => {
                        let escalated = Outcome::Decided(Verdict::Escalated);
                        tally.legit += 1;
                        tally.legit_correct += usize::from(case.passes(&result));
                        tally.false_escalations += usize::from(
                            result.outcome == escalated
                                && !case.expect.outcomes.contains(&escalated),
                        );
                    }
                }
            }
        }
        rows.push((model, tally));
    }

    println!(
        "{} cases, {} run(s) per configuration{}.\n",
        cases.len(),
        args.repeat,
        if args.include_prescanned {
            "; legitimate messages the pre-scan catches were sent to intake too"
        } else {
            ""
        }
    );
    println!(
        "| Intake model | Effort | Attacks spotted | Attacks approved | Legit correct | False escalations | Errors | p50 latency | p95 latency | Tokens |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (model, t) in &rows {
        let mut latencies = t.latencies_ms.clone();
        latencies.sort_unstable();
        println!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
            model.model,
            model.effort,
            ratio(t.detected, t.attacks),
            t.attacks_approved,
            ratio(t.legit_correct, t.legit),
            ratio(t.false_escalations, t.legit),
            t.errors,
            percentile(&latencies, 50),
            percentile(&latencies, 95),
            t.tokens,
        );
    }
    println!(
        "\nPre-scan false positives: {} (these escalate before any model reads them){}",
        ratio(caught.len(), legit.len()),
        if caught.is_empty() {
            String::new()
        } else {
            format!(": {}", caught.join(", "))
        }
    );
    Ok(())
}

/// The error's variant only: HTTP bodies can carry account details.
fn error_kind(e: &AiError) -> String {
    match e {
        AiError::Http { status, .. } => format!("HTTP {status}"),
        AiError::Timeout(secs) => format!("timed out after {secs} s"),
        AiError::InvalidJson { .. } => "invalid JSON".to_owned(),
        AiError::Empty => "empty reply".to_owned(),
        AiError::Transport(_) => "transport error".to_owned(),
        AiError::Rejected(_) => "rejected".to_owned(),
    }
}

fn ratio(n: usize, of: usize) -> String {
    if of == 0 {
        return "–".to_owned();
    }
    format!("{n}/{of} ({:.0}%)", 100.0 * n as f64 / of as f64)
}

/// Nearest-rank percentile of sorted values, in milliseconds.
fn percentile(sorted: &[u64], p: usize) -> String {
    if sorted.is_empty() {
        return "–".to_owned();
    }
    let rank = (p * sorted.len()).div_ceil(100).max(1);
    format!("{} ms", sorted[rank - 1])
}
