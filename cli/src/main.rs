//! `bdd` — spec-driven BDD/TDD CLI with an embedded MCP server.
//!
//! This binary is a composition root: it names concrete adapters and
//! wires them into application services, and nothing else.

use std::path::{Path, PathBuf};

use clap::{Args, Parser, Subcommand};

use bdd_cli::adapters::config::TomlModelStore;
use bdd_cli::adapters::console_prompt::ConsolePrompter;
use bdd_cli::adapters::fs_project::FsProjectFiles;
use bdd_cli::adapters::fs_scaffold::FsScaffoldWriter;
use bdd_cli::adapters::fs_sources::FsSourceFiles;
use bdd_cli::adapters::fs_spec::{FsFeatureFiles, FsSpecRepository};
use bdd_cli::adapters::fs_staging::FsChangeStore;
use bdd_cli::adapters::fs_state::FsStateStore;
use bdd_cli::adapters::gherkin_features::GherkinFeatureCatalog;
use bdd_cli::adapters::ollama::{
    DEFAULT_ENDPOINT, DEFAULT_GENERATION_TIMEOUT, OllamaCatalog, OllamaGenerator,
};
use bdd_cli::adapters::process_runtime::ProcessRuntimeProbe;
use bdd_cli::adapters::readline_prompt::ReadlinePrompter;
use bdd_cli::adapters::readline_shell::ReadlineShell;
use bdd_cli::adapters::runners::detect_runner;
use bdd_cli::adapters::spinner::Spinner;
use bdd_cli::application::change_service::ChangeService;
use bdd_cli::application::generation_service::{GenerationService, ResolvedLlm};
use bdd_cli::application::implement_service::ImplementService;
use bdd_cli::application::init_service::InitService;
use bdd_cli::application::inspect_service::InspectService;
use bdd_cli::application::model_service::{
    ModelResolution, ModelService, ModelSource, SessionModel,
};
use bdd_cli::application::scenario_service::ScenarioService;
use bdd_cli::application::spec_mutation_service::SpecMutationService;
use bdd_cli::application::spec_service::SpecService;
use bdd_cli::application::status_service::StatusService;
use bdd_cli::application::tdd_service::{TddError, TddService};
use bdd_cli::domain::language::{Language, detect_languages};
use bdd_cli::domain::tdd::ImplementAttempt;
use bdd_cli::greenfield::{DynLlm, Greenfield, parse_language, prompt_language};
use bdd_cli::ports::{FeatureCatalog as _, Prompter, TestFilter};
use bdd_cli::repl::{Ending, is_greenfield_start, offer_greenfield, run_shell};
use bdd_cli::workspace::{SPEC_PATH, workshop_layout};

#[derive(Parser)]
#[command(
    name = "bdd",
    version,
    about = "Spec-driven BDD/TDD authoring, validation, and execution"
)]
struct Cli {
    /// Override the configured LLM model for this invocation
    #[arg(long, global = true)]
    model: Option<String>,

    /// Project root (where requirements/ and .bdd-mcp.toml live)
    #[arg(long, global = true, default_value = ".")]
    root: PathBuf,

    /// Omitted entirely: print help and open the interactive shell
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold build files, Cucumber runner, empty spec, and config
    Init(InitArgs),
    /// Run the full orchestrated loop from zero (two human gates)
    Greenfield,
    /// Requirements spec tools (list, show, draft, validate, refine, mark-implemented)
    #[command(subcommand)]
    Spec(SpecCommand),
    /// Detect project languages, build system, runtimes, and roots
    Inspect,
    /// Feature discovery and creation
    #[command(subcommand)]
    Feature(FeatureCommand),
    /// Scenario mutations
    #[command(subcommand)]
    Scenario(ScenarioCommand),
    /// Step-definition discovery and generation
    #[command(subcommand)]
    Steps(StepsCommand),
    /// Unit-test generation (the TDD altitude)
    #[command(subcommand)]
    Unittest(UnittestCommand),
    /// Ask the model to make the failing tests pass (stages the files)
    Implement { req_id: String },
    /// Validate Gherkin and staged changes
    Validate,
    /// Run tests and update the RED/GREEN/REFACTOR phase (run_tests)
    Test(TestArgs),
    /// Show the current TDD phase and last run (get_tdd_state)
    State,
    /// Where every requirement stands on the road to implemented, and the next step
    Status,
    /// Begin a refactor step; only allowed on GREEN (start_refactor)
    Refactor(RefactorArgs),
    /// Staged-transaction management
    #[command(subcommand)]
    Changes(ChangesCommand),
    /// LLM model discovery and selection (Ollama)
    #[command(subcommand)]
    Model(ModelCommand),
    /// MCP server
    #[command(subcommand)]
    Mcp(McpCommand),
}

#[derive(Subcommand)]
enum SpecCommand {
    /// List every requirement with id, title, and status (list_requirements)
    List,
    /// Show one requirement, enriched with locations and a workflow hint (get_requirement)
    Show { req_id: String },
    /// Interactively draft a requirement (human input drives the spec)
    Draft,
    /// Validate the requirements spec on disk (validate_spec)
    Validate,
    /// Review one requirement's wording for quality (refine_requirement)
    Refine { req_id: String },
    /// Flip a requirement's status to implemented (requirement_mark_implemented)
    MarkImplemented { req_id: String },
}

#[derive(Subcommand)]
enum FeatureCommand {
    /// List every feature file with its name and scenario count
    List,
    /// Show one parsed feature file (path relative to --root)
    Show { path: String },
    /// Create a feature file (staged)
    Create {
        /// Feature file path relative to --root
        #[arg(long)]
        path: String,
        /// Feature name (the text after "Feature:")
        #[arg(long)]
        name: String,
    },
}

#[derive(Subcommand)]
enum ScenarioCommand {
    /// Append a tagged scenario to a feature file (staged)
    Add {
        /// Feature file path relative to --root
        #[arg(long)]
        feature: String,
        /// Requirement id the scenario implements (tagged @REQ-...)
        #[arg(long)]
        req: String,
        /// Scenario name
        #[arg(long)]
        name: String,
        /// One full Gherkin step per flag, e.g. --step "Given a calculator"
        #[arg(long = "step")]
        steps: Vec<String>,
    },
    /// Replace a scenario's steps and/or requirement tag (staged)
    Update {
        #[arg(long)]
        feature: String,
        #[arg(long)]
        name: String,
        /// New requirement id for the tag; omit to keep the current tag
        #[arg(long)]
        req: Option<String>,
        /// New steps; omit to keep the current steps
        #[arg(long = "step")]
        steps: Vec<String>,
    },
    /// Remove a scenario from a feature file (staged)
    Delete {
        #[arg(long)]
        feature: String,
        #[arg(long)]
        name: String,
    },
}

#[derive(Subcommand)]
enum StepsCommand {
    /// Report undefined and ambiguous steps (step_definitions_find)
    Missing,
    /// Generate step definitions for undefined steps (step_definition_create)
    Generate,
}

#[derive(Subcommand)]
enum UnittestCommand {
    /// Generate a unit test from a requirement's acceptance criteria (unit_test_create)
    Generate { req_id: String },
}

#[derive(Args)]
struct InitArgs {
    /// Target language (java, javascript, typescript, dotnet, rust); prompted when omitted
    #[arg(long)]
    language: Option<String>,
    /// Project name; defaults to the root directory's name
    #[arg(long)]
    name: Option<String>,
}

#[derive(Args)]
struct TestArgs {
    /// Run only one feature
    #[arg(long)]
    feature: Option<String>,
    /// Run only one scenario
    #[arg(long)]
    scenario: Option<String>,
}

#[derive(Args)]
struct RefactorArgs {
    /// What you intend to refactor and why
    #[arg(long)]
    note: Option<String>,
}

#[derive(Subcommand)]
enum ChangesCommand {
    Show,
    Commit,
    Discard,
}

#[derive(Subcommand)]
enum ModelCommand {
    /// List models available in Ollama
    List,
    /// Show the resolved model and where it came from (flag, config, discovery)
    Current,
    /// Persist a model choice in configuration
    Use { model_name: String },
}

#[derive(Subcommand)]
enum McpCommand {
    /// Serve the MCP tools over stdio
    Serve,
}

/// A command that already printed its (JSON) reply but must exit
/// nonzero — the structured `runtime_missing` refusal. A sentinel
/// instead of `process::exit` so the interactive shell survives it.
#[derive(Debug)]
struct NonzeroExit;

impl std::fmt::Display for NonzeroExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "nonzero exit")
    }
}

impl std::error::Error for NonzeroExit {}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(ref command) => match execute(&cli.root, cli.model.as_deref(), command) {
            Err(error) if error.is::<NonzeroExit>() => std::process::exit(1),
            other => other,
        },
        None => run_shell_mode(&cli.root, cli.model.as_deref()),
    }
}

fn execute(root: &Path, model: Option<&str>, command: &Command) -> anyhow::Result<()> {
    match command {
        Command::Spec(command) => run_spec(root, model, command),
        Command::Model(command) => run_model(root, model, command),
        Command::Inspect => {
            let service =
                InspectService::new(FsProjectFiles::new(root.to_path_buf()), ProcessRuntimeProbe);
            print_json(&service.inspect())
        }
        Command::Feature(command) => run_feature(root, command),
        Command::Scenario(command) => run_scenario(root, command),
        Command::Changes(command) => run_changes(root, command),
        Command::Validate => print_json(
            &change_service(root)
                .validate()
                .map_err(|e| anyhow::anyhow!(e.0))?,
        ),
        Command::Init(args) => run_init(root, args),
        Command::Greenfield => run_greenfield(root, model),
        Command::Mcp(McpCommand::Serve) => {
            let runtime = tokio::runtime::Runtime::new()?;
            runtime.block_on(bdd_cli::mcp::serve_stdio(root.to_path_buf()))
        }
        Command::Test(args) => run_test(root, args),
        Command::State => tdd_reply(tdd_service(root).state()),
        Command::Status => {
            let state = tdd_service(root)
                .state()
                .map_err(|e| anyhow::anyhow!(tdd_error_message(e)))?;
            let service = status_service(root, model)?;
            let report = service
                .status(&state.phase)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&report)?;
            // With a model resolved, the deterministic report is followed
            // by workflow-aware advice; a model failure never breaks status.
            if service.has_model() {
                const RED: &str = "\x1b[31m";
                const RESET: &str = "\x1b[0m";
                let work = Spinner::start("Asking the model for the next step - working");
                let advice = service.advice(&report, &state.last_run);
                drop(work);
                match advice {
                    Ok(Some(advice)) => println!("Model advice: {advice}"),
                    Ok(None) => {}
                    Err(e) => println!("{RED}{}{RESET}", e.0),
                }
            }
            Ok(())
        }
        Command::Refactor(args) => tdd_reply(tdd_service(root).refactor(args.note.as_deref())),
        Command::Steps(command) => {
            let service = generation_service(root, model)?;
            match command {
                StepsCommand::Missing => {
                    print_json(&service.steps_missing().map_err(|e| anyhow::anyhow!(e.0))?)
                }
                StepsCommand::Generate => {
                    print_json(&service.steps_generate().map_err(|e| anyhow::anyhow!(e.0))?)
                }
            }
        }
        Command::Implement { req_id } => {
            const RED: &str = "\x1b[31m";
            const GREEN: &str = "\x1b[32m";
            const RESET: &str = "\x1b[0m";
            let service = implement_service(root, model)?;
            let tdd = TddService::new(FsStateStore::new(root.to_path_buf()));
            let phase = tdd
                .state()
                .map_err(|e| anyhow::anyhow!(tdd_error_message(e)))?
                .phase;
            // The last run's failure details (stack traces included) are
            // the model's brief, together with every prior attempt on
            // this requirement; a fresh RED bar comes from bdd test
            // right before this. The attempt is logged so the next one
            // learns from it.
            let brief = tdd
                .implementation_brief(req_id)
                .map_err(|e| anyhow::anyhow!(tdd_error_message(e)))?;
            println!(
                "{req_id}: checking prerequisites - phase {phase}, \
                 {} recorded failure(s), {} prior attempt(s).",
                brief.failures.len(),
                brief.history.len()
            );
            let readiness = service
                .readiness(req_id, &phase, &brief.failures)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            for asset in &readiness.assets {
                let mark = if asset.present {
                    format!("{GREEN}present{RESET}")
                } else {
                    format!("{RED}missing{RESET}")
                };
                println!("  {}: {} - {mark}", asset.role, asset.path);
            }
            if !readiness.ready {
                for finding in &readiness.findings {
                    println!("{RED}{finding}{RESET}");
                }
                if service.has_model() {
                    let work =
                        Spinner::start("Asking the model whether implement can run - working");
                    let advice = service.advice(req_id, &readiness, &brief.failures);
                    drop(work);
                    match advice {
                        Ok(Some(advice)) => println!("Model advice: {advice}"),
                        Ok(None) => {}
                        Err(e) => println!("{RED}{}{RESET}", e.0),
                    }
                }
                return print_json(&readiness);
            }
            let work = Spinner::start(
                "Sending the sources, the failures, and the attempt history \
                 to the model - working",
            );
            let report = service
                .generate(req_id, &brief.failures, &brief.history, &brief.states)
                .map_err(|e| anyhow::anyhow!(e.0));
            drop(work);
            let report = report?;
            for target in &report.targets {
                println!("  staged: {target}");
            }
            if let Some(warning) = &report.warning {
                println!("{RED}{warning}{RESET}");
            }
            // The outcome stays empty until the next test run attaches
            // what these changes actually caused.
            tdd.record_attempt(ImplementAttempt {
                requirement: req_id.clone(),
                targets: report.targets.clone(),
                failures: brief.failures,
                ..Default::default()
            })
            .map_err(|e| anyhow::anyhow!(tdd_error_message(e)))?;
            print_json(&report)?;
            if report.staged && !report.targets.is_empty() {
                implement_follow_up(root, req_id)?;
            }
            Ok(())
        }
        Command::Unittest(UnittestCommand::Generate { req_id }) => {
            let service = generation_service(root, model)?;
            print_json(
                &service
                    .unittest_generate(req_id)
                    .map_err(|e| anyhow::anyhow!(e.0))?,
            )
        }
    }
}

/// Bare `bdd`: print the help, then hand the terminal to the
/// interactive shell. Each line is parsed exactly like a one-shot
/// invocation, inheriting the shell's --root and --model unless the
/// line sets its own; errors are printed and the shell keeps going.
/// Without a terminal (pipes, CI) the help alone is the whole reply.
/// The shell banner: the CLI mark as ASCII art - the red-to-green
/// cycle looping around the prompt - with the compiled-in version.
fn print_banner() {
    const R: &str = "\x1b[31m"; // the red arc
    const G: &str = "\x1b[32m"; // the green arc
    const B: &str = "\x1b[1m"; // bold
    const D: &str = "\x1b[2m"; // dim
    const X: &str = "\x1b[0m"; // reset
    // Interior width of the loop, in display columns.
    const W: usize = 34;
    let version = env!("CARGO_PKG_VERSION");
    let title = format!("> bdd  v{version}");
    let title_pad = " ".repeat(W - 4 - title.len());
    let top = "─".repeat(W);
    let gap = " ".repeat(W);
    println!();
    println!("  {R}╭{top}╮{X}");
    println!("  {R}│{X}{gap}{R}▼{X}");
    println!("  {R}│{X}    {B}> bdd{X}  {D}v{version}{X}{title_pad}{G}│{X}");
    println!("  {R}│{X}    {D}spec →{X} {R}RED{X} {D}→{X} {G}GREEN{X} {D}→ REFACTOR{X} {G}│{X}");
    println!("  {G}▲{X}{gap}{G}│{X}");
    println!("  {G}╰{top}╯{X}");
    println!();
}

/// The startup model line: what this session will use, or exactly what
/// to install to make generation work. Discovery is session-only - the
/// configuration is never touched until the user runs `bdd model use`.
/// Returns whether a model is ready, which gates the greenfield nudge.
fn announce_session_model(root: &Path, flag: Option<&str>) -> bool {
    let session = model_service(root).session_model(flag);
    let ready = matches!(session, SessionModel::Ready { .. });
    match session {
        SessionModel::Ready { model, source } => match source {
            ModelSource::Flag => println!("Model set: {model} (from the --model flag)."),
            ModelSource::Config => println!("Model set: {model} (from configuration)."),
            ModelSource::OnlyInstalled | ModelSource::FirstInstalled => println!(
                "Model set for this session: {model} (not saved - keep it with: \
                 bdd model use {model})."
            ),
        },
        SessionModel::NoModels => println!(
            "Ollama is running but has no models - generation will use \
             deterministic templates. For optimal results pull a model, \
             e.g.: ollama pull qwen3:8b"
        ),
        SessionModel::ProviderDown(_) => println!(
            "Ollama is not reachable - generation will use deterministic \
             templates. Install it from https://ollama.com, start it, and \
             pull a model, e.g.: ollama pull qwen3:8b"
        ),
    }
    ready
}

fn run_shell_mode(root: &Path, model: Option<&str>) -> anyhow::Result<()> {
    use clap::CommandFactory as _;
    use std::io::IsTerminal as _;
    Cli::command().print_help()?;
    if !std::io::stdin().is_terminal() {
        return Ok(());
    }
    print_banner();
    let model_ready = announce_session_model(root, model);
    println!(
        "Interactive shell - type commands without the bdd prefix \
         (e.g. spec list). exit, quit, or Ctrl+C leaves. The session \
         history lives in .bdd-history."
    );
    let history = root.join(".bdd-history");
    let first_session = !history.exists();
    let mut shell = ReadlineShell::open(history).map_err(|error| anyhow::anyhow!(error.0))?;
    let mut dispatch = |tokens: Vec<String>| {
        let explicit_root = tokens
            .iter()
            .any(|t| t == "--root" || t.starts_with("--root="));
        let argv = std::iter::once("bdd".to_string()).chain(tokens);
        match Cli::try_parse_from(argv) {
            Err(error) => {
                let _ = error.print();
            }
            Ok(cli) => match cli.command {
                None => {
                    let _ = Cli::command().print_help();
                }
                Some(command) => {
                    let line_root = if explicit_root { &cli.root } else { root };
                    let line_model = cli.model.as_deref().or(model);
                    match execute(line_root, line_model, &command) {
                        Ok(()) => {}
                        // The refusal already printed its JSON reply.
                        Err(error) if error.is::<NonzeroExit>() => {}
                        Err(error) => eprintln!("\x1b[31merror: {error}\x1b[0m"),
                    }
                }
            },
        }
    };
    let spec_exists = root.join(SPEC_PATH).exists();
    if is_greenfield_start(first_session, model_ready, spec_exists) {
        offer_greenfield(&mut shell, &mut dispatch);
    }
    let summary = run_shell(&mut shell, &mut dispatch);
    if let Ending::Failed(reason) = summary.ending {
        anyhow::bail!(reason);
    }
    println!(
        "Session over - {} command{} run.",
        summary.commands,
        if summary.commands == 1 { "" } else { "s" }
    );
    Ok(())
}

/// Detect the project's primary language. Authoring and discovery only
/// need markers, never runtimes.
fn primary_language(root: &Path) -> anyhow::Result<Language> {
    let files = FsProjectFiles::new(root.to_path_buf());
    detect_languages(&files).first().copied().ok_or_else(|| {
        anyhow::anyhow!(
            "No supported project detected (pom.xml, build.gradle, package.json, \
             *.csproj, Cargo.toml). Run bdd inspect."
        )
    })
}

fn generation_service(
    root: &Path,
    model_flag: Option<&str>,
) -> anyhow::Result<
    GenerationService<
        GherkinFeatureCatalog,
        FsSourceFiles,
        FsChangeStore,
        FsSpecRepository,
        OllamaGenerator,
    >,
> {
    let language = primary_language(root)?;
    let llm = resolved_ollama(root, model_flag)
        .map(|(model, generator)| ResolvedLlm { model, generator });
    Ok(GenerationService::new(
        GherkinFeatureCatalog::new(root.to_path_buf()),
        FsSourceFiles::new(root.to_path_buf()),
        FsChangeStore::new(root.to_path_buf()),
        FsSpecRepository::new(root.join(SPEC_PATH)),
        language,
        llm,
    ))
}

fn implement_service(
    root: &Path,
    model_flag: Option<&str>,
) -> anyhow::Result<
    ImplementService<
        GherkinFeatureCatalog,
        FsSourceFiles,
        FsChangeStore,
        FsSpecRepository,
        OllamaGenerator,
    >,
> {
    let language = primary_language(root)?;
    let llm = resolved_ollama(root, model_flag)
        .map(|(model, generator)| ResolvedLlm { model, generator });
    Ok(ImplementService::new(
        GherkinFeatureCatalog::new(root.to_path_buf()),
        FsSourceFiles::new(root.to_path_buf()),
        FsChangeStore::new(root.to_path_buf()),
        FsSpecRepository::new(root.join(SPEC_PATH)),
        language,
        llm,
    ))
}

fn status_service(
    root: &Path,
    model_flag: Option<&str>,
) -> anyhow::Result<
    StatusService<
        GherkinFeatureCatalog,
        FsSourceFiles,
        FsChangeStore,
        FsSpecRepository,
        OllamaGenerator,
    >,
> {
    let llm = resolved_ollama(root, model_flag)
        .map(|(model, generator)| ResolvedLlm { model, generator });
    Ok(StatusService::new(
        GherkinFeatureCatalog::new(root.to_path_buf()),
        FsSourceFiles::new(root.to_path_buf()),
        FsChangeStore::new(root.to_path_buf()),
        FsSpecRepository::new(root.join(SPEC_PATH)),
        primary_language(root)?,
        llm,
    ))
}

fn spec_service(root: &Path) -> SpecService<FsSpecRepository, FsFeatureFiles> {
    SpecService::new(
        FsSpecRepository::new(root.join(SPEC_PATH)),
        FsFeatureFiles::new(root.to_path_buf()),
        workshop_layout(),
    )
}

fn change_service(root: &Path) -> ChangeService<FsChangeStore, FsSpecRepository, FsFeatureFiles> {
    ChangeService::new(
        FsChangeStore::new(root.to_path_buf()),
        FsSpecRepository::new(root.join(SPEC_PATH)),
        FsFeatureFiles::new(root.to_path_buf()),
        SPEC_PATH.into(),
    )
}

fn mutation_service(
    root: &Path,
) -> SpecMutationService<
    FsSpecRepository,
    FsFeatureFiles,
    GherkinFeatureCatalog,
    FsChangeStore,
    FsStateStore,
> {
    SpecMutationService::new(
        FsSpecRepository::new(root.join(SPEC_PATH)),
        FsFeatureFiles::new(root.to_path_buf()),
        GherkinFeatureCatalog::new(root.to_path_buf()),
        FsChangeStore::new(root.to_path_buf()),
        FsStateStore::new(root.to_path_buf()),
        SPEC_PATH.into(),
    )
}

fn scenario_service(root: &Path) -> ScenarioService<FsChangeStore, GherkinFeatureCatalog> {
    ScenarioService::new(
        FsChangeStore::new(root.to_path_buf()),
        GherkinFeatureCatalog::new(root.to_path_buf()),
    )
}

fn tdd_service(root: &Path) -> TddService<FsStateStore> {
    TddService::new(FsStateStore::new(root.to_path_buf()))
}

fn run_test(root: &Path, args: &TestArgs) -> anyhow::Result<()> {
    // One shared language→runner dispatch (adapters::runners). The CLI
    // only executes when the language's runtime is present; the runner
    // enforces that with the structured `runtime_missing` refusal.
    let runner = detect_runner(root).map_err(|message| anyhow::anyhow!(message))?;
    let filter = TestFilter {
        feature: args.feature.clone(),
        scenario: args.scenario.clone(),
    };
    tdd_reply(tdd_service(root).run_tests(runner.as_ref(), &filter))
}

/// After `bdd implement` stages files, close the loop. On a terminal
/// the command offers to apply the staged changes and run the tests
/// right away; a decline - or piped stdin - still says the next
/// command in plain words instead of leaving it inside the JSON.
fn implement_follow_up(root: &Path, req_id: &str) -> anyhow::Result<()> {
    const RED: &str = "\x1b[31m";
    const GREEN: &str = "\x1b[32m";
    const RESET: &str = "\x1b[0m";
    use std::io::IsTerminal as _;
    let accepted = std::io::stdin().is_terminal()
        && ConsolePrompter::new(std::io::BufReader::new(std::io::stdin()), std::io::stdout())
            .confirm("Apply the staged files and run the tests now?")
            .unwrap_or(false);
    if !accepted {
        println!(
            "Next: {GREEN}changes commit && test{RESET} - then \
             {GREEN}implement {req_id}{RESET} again if the bar stays RED."
        );
        return Ok(());
    }
    let changes = change_service(root)
        .commit()
        .map_err(|e| anyhow::anyhow!(e.0))?;
    print_json(&changes)?;
    let runner = detect_runner(root).map_err(|message| anyhow::anyhow!(message))?;
    let filter = TestFilter {
        feature: None,
        scenario: None,
    };
    match tdd_service(root).run_tests(runner.as_ref(), &filter) {
        Ok(run) => {
            let green = run.phase == "GREEN";
            print_json(&run)?;
            if green {
                println!(
                    "{GREEN}GREEN{RESET} - next: refactor (optional), then \
                     {GREEN}spec mark-implemented {req_id} && changes commit{RESET}."
                );
            } else {
                println!(
                    "{RED}Still RED{RESET} - the fresh failures are recorded; \
                     run {GREEN}implement {req_id}{RESET} for another model \
                     attempt, or implement by hand and rerun test."
                );
            }
            Ok(())
        }
        Err(e) => tdd_reply::<serde_json::Value>(Err(e)),
    }
}

/// Print a TDD reply, turning a missing runtime into the structured
/// `runtime_missing` refusal with a nonzero exit (signalled, not
/// `process::exit`, so the interactive shell survives it).
fn tdd_reply<T: serde::Serialize>(result: Result<T, TddError>) -> anyhow::Result<()> {
    match result {
        Ok(report) => print_json(&report),
        Err(TddError::RuntimeMissing { runtime, hint }) => {
            print_json(&serde_json::json!({
                "error": "runtime_missing",
                "runtime": runtime,
                "hint": hint,
            }))?;
            Err(NonzeroExit.into())
        }
        Err(TddError::Other(message)) => anyhow::bail!(message),
    }
}

/// Flatten a TDD error into its human message, for commands whose reply
/// is not a TDD report.
fn tdd_error_message(error: TddError) -> String {
    match error {
        TddError::Other(message) => message,
        TddError::RuntimeMissing { hint, .. } => hint,
    }
}

fn run_changes(root: &Path, command: &ChangesCommand) -> anyhow::Result<()> {
    let service = change_service(root);
    let report = match command {
        ChangesCommand::Show => service.show(),
        ChangesCommand::Commit => service.commit(),
        ChangesCommand::Discard => service.discard(),
    }
    .map_err(|e| anyhow::anyhow!(e.0))?;
    print_json(&report)
}

fn run_scenario(root: &Path, command: &ScenarioCommand) -> anyhow::Result<()> {
    let service = scenario_service(root);
    let report = match command {
        ScenarioCommand::Add {
            feature,
            req,
            name,
            steps,
        } => service.add_scenario(feature, req, name, steps.clone()),
        ScenarioCommand::Update {
            feature,
            name,
            req,
            steps,
        } => service.update_scenario(feature, name, steps.clone(), req.as_deref()),
        ScenarioCommand::Delete { feature, name } => service.delete_scenario(feature, name),
    }
    .map_err(|e| anyhow::anyhow!(e.0))?;
    print_json(&report)
}

fn run_spec(root: &Path, model: Option<&str>, command: &SpecCommand) -> anyhow::Result<()> {
    let service = spec_service(root);
    match command {
        SpecCommand::List => {
            let requirements = service
                .list_requirements()
                .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&requirements)
        }
        SpecCommand::Show { req_id } => {
            let requirement = service
                .get_requirement(req_id)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&requirement)
        }
        SpecCommand::Validate => print_json(&service.validate_spec()),
        SpecCommand::Refine { req_id } => {
            let report = service
                .refine_requirement(req_id)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&report)
        }
        SpecCommand::Draft => {
            let mut prompter = interactive_prompter();
            let service = mutation_service(root);
            let report = match resolved_ollama(root, model) {
                Some((name, generator)) => {
                    service.draft_assisted(prompter.as_mut(), &name, &generator)
                }
                None => service.draft(prompter.as_mut()),
            }
            .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&report)
        }
        SpecCommand::MarkImplemented { req_id } => {
            let report = mutation_service(root)
                .mark_implemented(req_id)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&report)
        }
    }
}

fn config_file(root: &Path) -> PathBuf {
    root.join(".bdd-mcp.toml")
}

fn run_feature(root: &Path, command: &FeatureCommand) -> anyhow::Result<()> {
    let catalog = GherkinFeatureCatalog::new(root.to_path_buf());
    match command {
        FeatureCommand::List => {
            let summaries = catalog.list().map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&summaries)
        }
        FeatureCommand::Show { path } => {
            let doc = catalog.read(path).map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&doc)
        }
        FeatureCommand::Create { path, name } => {
            let report = scenario_service(root)
                .create_feature(path, name)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            print_json(&report)
        }
    }
}

fn model_service(root: &Path) -> ModelService<OllamaCatalog, TomlModelStore> {
    let store = TomlModelStore::new(config_file(root));
    let endpoint = store
        .endpoint()
        .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
    ModelService::new(OllamaCatalog::new(endpoint), store)
}

/// The one place a model flag becomes a live Ollama generator, shared by
/// generation and greenfield. Hybrid generation: templates always work,
/// a resolved model only improves them, so any resolution problem
/// silently means "no LLM".
fn resolved_ollama(root: &Path, model_flag: Option<&str>) -> Option<(String, OllamaGenerator)> {
    match model_service(root).resolve(model_flag) {
        ModelResolution::Resolved { model, .. } => {
            let store = TomlModelStore::new(config_file(root));
            let endpoint = store
                .endpoint()
                .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
            let timeout = store
                .timeout_seconds()
                .map_or(DEFAULT_GENERATION_TIMEOUT, std::time::Duration::from_secs);
            Some((model, OllamaGenerator::with_timeout(endpoint, timeout)))
        }
        ModelResolution::Unavailable(_) => None,
    }
}

fn run_model(root: &Path, flag: Option<&str>, command: &ModelCommand) -> anyhow::Result<()> {
    let service = model_service(root);
    match command {
        ModelCommand::List => {
            let models = service.list().map_err(|e| anyhow::anyhow!(e.0))?;
            if models.is_empty() {
                println!("No models installed - pull one first (e.g. `ollama pull`).");
                return Ok(());
            }
            for model in models {
                let size = model
                    .size_bytes
                    .map(|b| format!("{:.1} GB", b as f64 / 1_000_000_000.0))
                    .unwrap_or_else(|| "-".to_string());
                let modified = model.modified_at.as_deref().unwrap_or("-");
                println!("{}\t{}\t{}", model.name, size, modified);
            }
            Ok(())
        }
        ModelCommand::Current => match service.resolve(flag) {
            ModelResolution::Resolved { model, source } => {
                let source = match source {
                    ModelSource::Flag => "--model flag",
                    ModelSource::Config => "configuration",
                    ModelSource::OnlyInstalled => "the only installed model",
                    ModelSource::FirstInstalled => {
                        "the first installed model, this session only - \
                         persist it with: bdd model use <model-name>"
                    }
                };
                println!("{model} (from {source})");
                Ok(())
            }
            ModelResolution::Unavailable(message) => anyhow::bail!(message),
        },
        ModelCommand::Use { model_name } => {
            service
                .choose(model_name)
                .map_err(|e| anyhow::anyhow!(e.0))?;
            let file = config_file(root);
            // Canonicalize after the write so the user sees the real
            // absolute location, not the raw --root-relative path.
            let shown = file.canonicalize().unwrap_or(file);
            println!("Configured model: {model_name}");
            println!("Written to: {}", shown.display());
            Ok(())
        }
    }
}

fn run_init(root: &Path, args: &InitArgs) -> anyhow::Result<()> {
    let language = match &args.language {
        Some(answer) => parse_language(answer).ok_or_else(|| {
            anyhow::anyhow!(
                "unknown language {answer:?} - pick java, javascript, typescript, dotnet, or rust"
            )
        })?,
        None => {
            let mut prompter = interactive_prompter();
            prompt_language(prompter.as_mut()).map_err(|e| anyhow::anyhow!(e.0))?
        }
    };
    let name = match &args.name {
        Some(name) => name.clone(),
        None => root
            .canonicalize()
            .ok()
            .and_then(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "project".into()),
    };
    let report = InitService::new(FsScaffoldWriter::new(root.to_path_buf()))
        .init(language, &name)
        .map_err(|e| anyhow::anyhow!(e.0))?;
    print_json(&report)
}

fn run_greenfield(root: &Path, model_flag: Option<&str>) -> anyhow::Result<()> {
    let llm = resolved_ollama(root, model_flag)
        .map(|(model, generator)| (model, DynLlm(std::sync::Arc::new(generator))));
    let mut prompter = interactive_prompter();
    let report = Greenfield::new(root.to_path_buf(), llm)
        .run(prompter.as_mut())
        .map_err(|message| anyhow::anyhow!(message))?;
    print_json(&report)
}

/// The wizard prompter. On a real terminal, rustyline gives the answers
/// full line editing - arrow keys move the cursor anywhere in the typed
/// text, Home/End jump, up-arrow recalls this session's answers. Piped
/// input (scripts, CI) falls back to plain buffered reads.
fn interactive_prompter() -> Box<dyn Prompter> {
    use std::io::IsTerminal as _;
    if std::io::stdin().is_terminal()
        && let Ok(prompter) = ReadlinePrompter::new()
    {
        return Box::new(prompter);
    }
    Box::new(ConsolePrompter::new(
        std::io::BufReader::new(std::io::stdin()),
        std::io::stdout(),
    ))
}

fn print_json<T: serde::Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
