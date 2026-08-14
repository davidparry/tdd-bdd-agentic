package com.davidparry.workshop.mcp.client;

import java.nio.file.Files;
import java.nio.file.Path;

/**
 * The workshop's agent harness: an MCP client that launches the workflow
 * server as a child process (stdio transport), discovers its tools, and walks
 * one full pass of the agentic spec-to-green loop (SDD -> BDD -> TDD) —
 * narrating the protocol exchange so you can see what an IDE or LLM host
 * does under the hood.
 *
 * <p>This class is the composition root and nothing else: it resolves paths
 * and wires {@link AgentWorkflow} to the real server through
 * {@link SdkToolClient}. All walkthrough logic lives in {@link AgentWorkflow},
 * which is covered at 100% against a scripted fake.
 *
 * <p>Run it from the repo root after {@code mvn package}:
 * <pre>{@code java -jar mcp-client/target/tdd-agent.jar}</pre>
 */
public final class TddAgent {

    private TddAgent() {
    }

    public static void main(String[] args) {
        Path root = resolveWorkshopRoot();
        Path serverJar = root.resolve("mcp-server/target/tdd-mcp-server.jar");
        if (!Files.exists(serverJar)) {
            System.out.println("Server jar not found: " + serverJar);
            System.out.println("Build it first: mvn -q package  (from the repo root)");
            System.exit(1);
        }

        Narrator narrator = new Narrator(System.out::println);
        narrator.banner("STEP 0 — Launch the server");
        narrator.say("The client starts the MCP server as a child process and talks JSON-RPC 2.0 over stdin/stdout.");
        narrator.say("Command: java -Dworkshop.root=" + root + " -jar " + root.relativize(serverJar));

        new AgentWorkflow(new SdkToolClient(root, serverJar), narrator).run();
    }

    private static Path resolveWorkshopRoot() {
        String configured = System.getProperty("workshop.root", System.getenv("WORKSHOP_ROOT"));
        Path root = configured != null
                ? Path.of(configured)
                : Path.of("").toAbsolutePath();
        return root.toAbsolutePath().normalize();
    }
}
