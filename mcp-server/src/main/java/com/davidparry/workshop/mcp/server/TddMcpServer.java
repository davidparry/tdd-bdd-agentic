package com.davidparry.workshop.mcp.server;

import com.davidparry.workshop.mcp.server.requirements.RequirementRefiner;
import com.davidparry.workshop.mcp.server.requirements.RequirementsRepository;
import com.davidparry.workshop.mcp.server.requirements.SpecValidator;
import com.davidparry.workshop.mcp.server.tdd.MavenTestRunner;
import com.davidparry.workshop.mcp.server.tdd.TddStateMachine;

import io.modelcontextprotocol.json.McpJsonDefaults;
import io.modelcontextprotocol.server.McpSyncServer;
import io.modelcontextprotocol.server.transport.StdioServerTransportProvider;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.concurrent.CountDownLatch;

/**
 * The composition root of the workshop MCP server. Everything here is
 * process-boundary code: reading the environment, wiring the object graph,
 * binding the stdio transport, and blocking until the JVM shuts down.
 *
 * <p>The tool schemas and server assembly live in {@link McpServerFactory};
 * the tool logic lives in {@link WorkflowToolHandlers}. Both are fully unit
 * tested — this class is deliberately too thin to need tests.
 *
 * <p>IMPORTANT: a stdio MCP server must never write to stdout — that would
 * corrupt the JSON-RPC stream. All diagnostics go to stderr.
 */
public final class TddMcpServer {

    private TddMcpServer() {
    }

    public static void main(String[] args) throws InterruptedException {
        Path root = resolveWorkshopRoot();
        Path requirementsFile = root.resolve("requirements").resolve("requirements.json");
        if (!Files.exists(requirementsFile)) {
            System.err.println("[tdd-mcp-server] Requirements file not found: " + requirementsFile);
            System.err.println("[tdd-mcp-server] Set -Dworkshop.root=/path/to/tdd-bdd-agentic or run from the repo root.");
            System.exit(1);
        }

        WorkflowToolHandlers handlers = new WorkflowToolHandlers(
                () -> RequirementsRepository.load(requirementsFile),
                new SpecValidator(requirementsFile, root),
                new RequirementRefiner(),
                new MavenTestRunner(root),
                new TddStateMachine());

        McpSyncServer server = McpServerFactory.create(
                new StdioServerTransportProvider(McpJsonDefaults.getMapper()),
                handlers);

        System.err.println("[tdd-mcp-server] Started. Workshop root: " + root);
        System.err.println("[tdd-mcp-server] Waiting for an MCP client on stdio...");

        CountDownLatch shutdown = new CountDownLatch(1);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            server.close();
            shutdown.countDown();
        }));
        shutdown.await();
    }

    private static Path resolveWorkshopRoot() {
        String configured = System.getProperty("workshop.root", System.getenv("WORKSHOP_ROOT"));
        Path root = configured != null
                ? Path.of(configured)
                : Path.of("").toAbsolutePath();
        return root.toAbsolutePath().normalize();
    }
}
