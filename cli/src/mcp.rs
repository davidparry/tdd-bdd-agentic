//! The embedded MCP server: `bdd mcp serve` exposes the workflow over
//! stdio. The seven frozen tools carry the workshop server's exact names,
//! schemas, and reply shapes so existing clients keep working; the
//! additive typed tools expose the CLI's inspection and staged-mutation
//! surface. This module is a delivery mechanism like `main.rs`: it wires
//! the same application services onto a transport, so it is allowed to
//! name concrete adapters. The test-runner factory is injectable so every
//! reply path is testable without real runtimes.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock, ServerCapabilities, ServerInfo};
use rmcp::{ErrorData as McpError, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::adapters::fs_project::FsProjectFiles;
use crate::adapters::fs_spec::{FsFeatureFiles, FsSpecRepository};
use crate::adapters::fs_staging::FsChangeStore;
use crate::adapters::fs_state::FsStateStore;
use crate::adapters::gherkin_features::GherkinFeatureCatalog;
use crate::adapters::process_exec::ProcessCommandExecutor;
use crate::adapters::process_runtime::ProcessRuntimeProbe;
use crate::adapters::runners::detect_runner;
use crate::application::change_service::ChangeService;
use crate::application::command_service::CommandService;
use crate::application::inspect_service::InspectService;
use crate::application::scenario_service::ScenarioService;
use crate::application::spec_service::SpecService;
use crate::application::tdd_service::{TddError, TddService};
use crate::ports::{FeatureCatalog as _, SpecRepository as _, TestFilter, TestRunner};
use crate::workspace::{SPEC_PATH, workshop_layout};

type RunnerFactory = Arc<dyn Fn(&Path) -> Result<Box<dyn TestRunner>, String> + Send + Sync>;

#[derive(Deserialize, JsonSchema)]
pub struct IdParam {
    /// The requirement id, e.g. REQ-003
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct NoteParam {
    /// What you intend to refactor and why
    pub note: Option<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct FeaturePathParam {
    /// Feature file path relative to the project root
    pub path: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct FeatureCreateParams {
    /// Feature file path relative to the project root
    pub path: String,
    /// Feature name (the text after "Feature:")
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ScenarioAddParams {
    /// Feature file path relative to the project root
    pub feature: String,
    /// Requirement id the scenario implements (tagged @REQ-...)
    pub req: String,
    /// Scenario name
    pub name: String,
    /// Full Gherkin steps, e.g. "Given a calculator"
    pub steps: Vec<String>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ScenarioUpdateParams {
    /// Feature file path relative to the project root
    pub feature: String,
    /// Scenario name
    pub name: String,
    /// New requirement id for the tag; omit to keep the current tag
    pub req: Option<String>,
    /// New steps; omit to keep the current steps
    pub steps: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ScenarioDeleteParams {
    /// Feature file path relative to the project root
    pub feature: String,
    /// Scenario name
    pub name: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct CommandRunParams {
    /// The command as argv: the program first, then its arguments. No
    /// shell is involved, so pipes, redirection, and chaining do not work.
    pub command: Vec<String>,
    /// Timeout in seconds; default and maximum 300.
    pub timeout_secs: Option<u64>,
}

/// The MCP delivery of the workflow: seven frozen tools plus the
/// additive typed tools, all backed by the same application services the
/// CLI commands use.
#[derive(Clone)]
pub struct WorkflowServer {
    root: PathBuf,
    runner_factory: RunnerFactory,
    // Read by the `#[tool_handler]`-generated `ServerHandler` impl.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

/// Logs a tool invocation and opens a span so the result events emitted
/// by `json_result`/`error_result` carry the tool name. Safe to hold in
/// the async handlers because none of them await while it is alive.
fn tool_call(tool: &'static str) -> tracing::span::EnteredSpan {
    let span = tracing::info_span!("mcp", tool);
    let entered = span.entered();
    tracing::info!("MCP tool invoked");
    entered
}

fn json_result<T: serde::Serialize>(body: &T) -> CallToolResult {
    let text = serde_json::to_string_pretty(body).expect("reports serialize");
    tracing::debug!(result = %text, "MCP tool result");
    CallToolResult::success(vec![ContentBlock::text(text)])
}

fn error_result(message: &str) -> CallToolResult {
    tracing::warn!(error = %message, "MCP tool error");
    CallToolResult::error(vec![ContentBlock::text(message.to_string())])
}

/// A TDD-loop failure as a tool reply: missing runtimes keep the CLI's
/// structured `runtime_missing` shape, everything else is the message.
fn tdd_error_result(error: TddError) -> CallToolResult {
    match error {
        TddError::RuntimeMissing { runtime, hint } => error_result(
            &serde_json::to_string_pretty(&serde_json::json!({
                "error": "runtime_missing",
                "runtime": runtime,
                "hint": hint,
            }))
            .expect("a literal object serializes"),
        ),
        TddError::Other(message) => error_result(&message),
    }
}

#[tool_router]
impl WorkflowServer {
    pub fn new(root: PathBuf) -> Self {
        Self::with_runner_factory(root, Arc::new(detect_runner))
    }

    /// Constructor for tests: inject a scripted runner factory so every
    /// `run_tests` reply path is reachable without real runtimes.
    pub fn with_runner_factory(root: PathBuf, runner_factory: RunnerFactory) -> Self {
        Self {
            root,
            runner_factory,
            tool_router: Self::tool_router(),
        }
    }

    fn spec_repository(&self) -> FsSpecRepository {
        FsSpecRepository::new(self.root.join(SPEC_PATH))
    }

    fn spec_service(&self) -> SpecService<FsSpecRepository, FsFeatureFiles> {
        SpecService::new(
            self.spec_repository(),
            FsFeatureFiles::new(self.root.clone()),
            workshop_layout(),
        )
    }

    fn scenario_service(&self) -> ScenarioService<FsChangeStore, GherkinFeatureCatalog> {
        ScenarioService::new(
            FsChangeStore::new(self.root.clone()),
            GherkinFeatureCatalog::new(self.root.clone()),
        )
    }

    fn change_service(&self) -> ChangeService<FsChangeStore, FsSpecRepository, FsFeatureFiles> {
        ChangeService::new(
            FsChangeStore::new(self.root.clone()),
            self.spec_repository(),
            FsFeatureFiles::new(self.root.clone()),
            SPEC_PATH.into(),
        )
    }

    fn tdd_service(&self) -> TddService<FsStateStore> {
        TddService::new(FsStateStore::new(self.root.clone()))
    }

    // ---- the seven frozen tools (workshop server contract) ----------------

    #[tool(
        description = "List every requirement of the kata with its id, title, and \
        implementation status. Use this to find pending work."
    )]
    async fn list_requirements(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("list_requirements");
        // Typed body (not json!) so the field order matches the Java
        // server's LinkedHashMap output: project, then id/title/status.
        #[derive(serde::Serialize)]
        struct Row<'a> {
            id: &'a str,
            title: &'a str,
            status: &'a str,
        }
        #[derive(serde::Serialize)]
        struct Body<'a> {
            project: &'a str,
            requirements: Vec<Row<'a>>,
        }
        Ok(match self.spec_repository().load() {
            Err(e) => error_result(&e.0),
            Ok(spec) => json_result(&Body {
                project: &spec.project,
                requirements: spec
                    .requirements
                    .iter()
                    .map(|r| Row {
                        id: &r.id,
                        title: &r.title,
                        status: &r.status,
                    })
                    .collect(),
            }),
        })
    }

    #[tool(
        description = "Get the user story and acceptance criteria for one requirement. \
        Turn each acceptance criterion into a failing test before writing production code."
    )]
    async fn get_requirement(
        &self,
        Parameters(params): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("get_requirement");
        tracing::debug!(id = %params.id, "tool arguments");
        Ok(match self.spec_service().get_requirement(&params.id) {
            Ok(requirement) => json_result(&requirement),
            Err(e) => error_result(&e.0),
        })
    }

    #[tool(
        description = "Validate the requirements spec on disk. Call this after every edit \
        to the requirements file and fix the reported issues until valid is true — only a \
        valid spec is worth turning into scenarios and code. Implemented requirements must \
        have tagged Gherkin scenarios in their feature file."
    )]
    async fn validate_spec(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("validate_spec");
        Ok(json_result(&self.spec_service().validate_spec()))
    }

    #[tool(
        description = "Review one requirement's wording for quality: ambiguous language, a \
        story missing its actor or rationale, outcomes that are not measurable, criteria \
        covering more than one action, and missing edge cases. Reword the requirement from \
        the findings and call again - iterate until there are no findings, then have the \
        developer approve the wording before writing any scenario."
    )]
    async fn refine_requirement(
        &self,
        Parameters(params): Parameters<IdParam>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("refine_requirement");
        tracing::debug!(id = %params.id, "tool arguments");
        Ok(match self.spec_service().refine_requirement(&params.id) {
            Ok(report) => json_result(&report),
            Err(e) => error_result(&e.0),
        })
    }

    #[tool(
        description = "Run the project test suite and report the outcome. Updates the \
        Red/Green/Refactor state: failures mean RED, all-passing means GREEN."
    )]
    async fn run_tests(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("run_tests");
        Ok(match (self.runner_factory)(&self.root) {
            Err(message) => error_result(&message),
            Ok(runner) => {
                match self
                    .tdd_service()
                    .run_tests(runner.as_ref(), &TestFilter::default())
                {
                    Ok(report) => json_result(&report),
                    Err(error) => tdd_error_result(error),
                }
            }
        })
    }

    #[tool(
        description = "Get the current phase of the Red/Green/Refactor cycle, the last \
        test run summary, interpretation instructions, at most the three latest dated \
        state entries, and a suggested next step."
    )]
    async fn get_tdd_state(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("get_tdd_state");
        Ok(match self.tdd_service().state() {
            Ok(report) => json_result(&report),
            Err(error) => tdd_error_result(error),
        })
    }

    #[tool(
        description = "Begin a refactor step. Only allowed when the bar is GREEN — never \
        refactor on failing tests. Run run_tests afterwards to prove the refactor was safe."
    )]
    async fn start_refactor(
        &self,
        Parameters(params): Parameters<NoteParam>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("start_refactor");
        tracing::debug!(note = ?params.note, "tool arguments");
        Ok(match self.tdd_service().refactor(params.note.as_deref()) {
            Ok(report) => json_result(&report),
            Err(error) => tdd_error_result(error),
        })
    }

    // ---- additive typed tools ---------------------------------------------

    #[tool(
        description = "Detect the project's languages, BDD frameworks, runtimes, and \
        whether test execution is possible. Authoring never requires a runtime."
    )]
    async fn project_inspect(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("project_inspect");
        let service =
            InspectService::new(FsProjectFiles::new(self.root.clone()), ProcessRuntimeProbe);
        Ok(json_result(&service.inspect()))
    }

    #[tool(description = "List every Gherkin feature file with its name and scenario count.")]
    async fn feature_list(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("feature_list");
        Ok(match GherkinFeatureCatalog::new(self.root.clone()).list() {
            Ok(summaries) => json_result(&summaries),
            Err(e) => error_result(&e.0),
        })
    }

    #[tool(description = "Read one parsed feature file: tags, scenarios, and steps.")]
    async fn feature_read(
        &self,
        Parameters(params): Parameters<FeaturePathParam>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("feature_read");
        tracing::debug!(path = %params.path, "tool arguments");
        Ok(
            match GherkinFeatureCatalog::new(self.root.clone()).read(&params.path) {
                Ok(doc) => json_result(&doc),
                Err(e) => error_result(&e.0),
            },
        )
    }

    #[tool(description = "Create an empty feature file (staged; apply with changes_commit).")]
    async fn feature_create(
        &self,
        Parameters(params): Parameters<FeatureCreateParams>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("feature_create");
        tracing::debug!(path = %params.path, name = %params.name, "tool arguments");
        Ok(
            match self
                .scenario_service()
                .create_feature(&params.path, &params.name)
            {
                Ok(report) => json_result(&report),
                Err(e) => error_result(&e.0),
            },
        )
    }

    #[tool(
        description = "Append a scenario tagged @REQ-... to a feature file (staged; \
        apply with changes_commit). Steps must be full Gherkin lines."
    )]
    async fn scenario_add(
        &self,
        Parameters(params): Parameters<ScenarioAddParams>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("scenario_add");
        tracing::debug!(feature = %params.feature, name = %params.name, "tool arguments");
        Ok(
            match self.scenario_service().add_scenario(
                &params.feature,
                &params.req,
                &params.name,
                params.steps,
            ) {
                Ok(report) => json_result(&report),
                Err(e) => error_result(&e.0),
            },
        )
    }

    #[tool(
        description = "Replace a scenario's steps and/or requirement tag (staged; apply \
        with changes_commit)."
    )]
    async fn scenario_update(
        &self,
        Parameters(params): Parameters<ScenarioUpdateParams>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("scenario_update");
        tracing::debug!(feature = %params.feature, name = %params.name, "tool arguments");
        Ok(
            match self.scenario_service().update_scenario(
                &params.feature,
                &params.name,
                params.steps.unwrap_or_default(),
                params.req.as_deref(),
            ) {
                Ok(report) => json_result(&report),
                Err(e) => error_result(&e.0),
            },
        )
    }

    #[tool(
        description = "Remove a scenario from a feature file (staged; apply with \
        changes_commit)."
    )]
    async fn scenario_delete(
        &self,
        Parameters(params): Parameters<ScenarioDeleteParams>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("scenario_delete");
        tracing::debug!(feature = %params.feature, name = %params.name, "tool arguments");
        Ok(
            match self
                .scenario_service()
                .delete_scenario(&params.feature, &params.name)
            {
                Ok(report) => json_result(&report),
                Err(e) => error_result(&e.0),
            },
        )
    }

    #[tool(description = "Show every staged change waiting for review.")]
    async fn changes_show(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("changes_show");
        Ok(match self.change_service().show() {
            Ok(report) => json_result(&report),
            Err(e) => error_result(&e.0),
        })
    }

    #[tool(description = "Apply every staged change to the working tree and clear the area.")]
    async fn changes_commit(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("changes_commit");
        Ok(match self.change_service().commit() {
            Ok(report) => json_result(&report),
            Err(e) => error_result(&e.0),
        })
    }

    #[tool(description = "Drop every staged change without applying it.")]
    async fn changes_discard(&self) -> Result<CallToolResult, McpError> {
        let _span = tool_call("changes_discard");
        Ok(match self.change_service().discard() {
            Ok(report) => json_result(&report),
            Err(e) => error_result(&e.0),
        })
    }

    #[tool(
        description = "Run one allowlisted dev-tool command (cargo, mvn, npm, npx, node, \
        dotnet, java, javac, tsc) inside the project root during the implementation \
        phase — the bar must be RED. The command executes directly as argv with no \
        shell, so pipes, redirection, and chaining are inert; arguments may not be \
        absolute paths or contain '..'. Use it to build, compile, or install what the \
        failing tests need, then call run_tests."
    )]
    async fn command_run(
        &self,
        Parameters(params): Parameters<CommandRunParams>,
    ) -> Result<CallToolResult, McpError> {
        let _span = tool_call("command_run");
        tracing::debug!(command = %params.command.join(" "), "tool arguments");
        let service = CommandService::new(
            FsStateStore::new(self.root.clone()),
            ProcessCommandExecutor,
            self.root.clone(),
        );
        Ok(match service.run(&params.command, params.timeout_secs) {
            Ok(report) => json_result(&report),
            Err(e) => error_result(&e.0),
        })
    }
}

#[tool_handler]
impl ServerHandler for WorkflowServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.server_info.name = "tdd-workflow-server".into();
        info.server_info.version = "1.0.0".into();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.instructions = Some(
            "Drives a spec-driven TDD/BDD workflow. The requirements spec is the source \
             of truth and the entry point. Spec iteration loop: draft or edit a \
             requirement in the requirements file -> validate_spec until the spec is \
             valid -> refine_requirement on the new or changed requirement and reword \
             from its findings until there are none -> have the developer approve the \
             wording. From an approved spec: list_requirements -> get_requirement (pick \
             a pending one) -> write a Gherkin scenario from its acceptance criteria in \
             the feature file (BDD level), add step definitions if needed, and/or a \
             failing unit test -> run_tests (expect RED) -> implement (command_run \
             can build or install what the failing tests need; it only works on a \
             RED bar) -> run_tests (expect GREEN) -> start_refactor -> run_tests. \
             The human developer stays in control of every engineering decision."
                .to_string(),
        );
        info
    }
}

/// Serve the workflow over stdio until the client disconnects.
pub async fn serve_stdio(root: PathBuf) -> anyhow::Result<()> {
    use rmcp::ServiceExt as _;
    tracing::info!(root = %root.display(), "MCP server starting on stdio");
    let service = WorkflowServer::new(root)
        .serve((tokio::io::stdin(), tokio::io::stdout()))
        .await?;
    service.waiting().await?;
    tracing::info!("MCP server stopped (client disconnected)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn a_runner_is_detected_for_every_supported_marker() {
        for marker in ["pom.xml", "package.json", "Cargo.toml", "app.csproj"] {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join(marker), "").unwrap();
            assert!(detect_runner(dir.path()).is_ok(), "no runner for {marker}");
        }
        let typescript = tempfile::tempdir().unwrap();
        fs::write(typescript.path().join("package.json"), "{}").unwrap();
        fs::write(typescript.path().join("tsconfig.json"), "{}").unwrap();
        assert!(detect_runner(typescript.path()).is_ok());
    }

    #[test]
    fn an_unrecognized_directory_has_no_runner() {
        let dir = tempfile::tempdir().unwrap();
        let error = detect_runner(dir.path())
            .err()
            .expect("no runner in an empty directory");
        assert!(
            error.starts_with("No supported project detected"),
            "got: {error}"
        );
    }

    #[test]
    fn a_missing_runtime_becomes_the_structured_refusal() {
        let result = tdd_error_result(TddError::RuntimeMissing {
            runtime: "mvn".into(),
            hint: "Install Maven.".into(),
        });
        assert_eq!(result.is_error, Some(true));
        let text = result.content[0].as_text().unwrap();
        assert!(text.text.contains("\"error\": \"runtime_missing\""));
        assert!(text.text.contains("Install Maven."));
    }

    #[test]
    fn other_tdd_errors_become_plain_error_text() {
        let result = tdd_error_result(TddError::Other("Never refactor on a red bar".into()));
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result.content[0].as_text().unwrap().text,
            "Never refactor on a red bar"
        );
    }
}
