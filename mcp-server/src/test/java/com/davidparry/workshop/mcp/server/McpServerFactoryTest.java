package com.davidparry.workshop.mcp.server;

import com.davidparry.workshop.mcp.server.requirements.RequirementRefiner;
import com.davidparry.workshop.mcp.server.requirements.RequirementsRepository;
import com.davidparry.workshop.mcp.server.requirements.SpecValidator;
import com.davidparry.workshop.mcp.server.tdd.TddStateMachine;
import com.davidparry.workshop.mcp.server.tdd.TestRunSummary;

import io.modelcontextprotocol.server.McpServerFeatures.SyncToolSpecification;
import io.modelcontextprotocol.server.McpSyncServer;
import io.modelcontextprotocol.spec.McpSchema;
import io.modelcontextprotocol.spec.McpSchema.CallToolRequest;
import io.modelcontextprotocol.spec.McpSchema.CallToolResult;
import io.modelcontextprotocol.spec.McpServerSession;
import io.modelcontextprotocol.spec.McpServerTransportProvider;

import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import reactor.core.publisher.Mono;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static org.assertj.core.api.Assertions.assertThat;

class McpServerFactoryTest {

    @TempDir
    Path dir;

    private WorkflowToolHandlers handlers;

    /** A transport that never opens a session — assembly needs no stdio. */
    private static final class FakeTransportProvider implements McpServerTransportProvider {
        @Override
        public void setSessionFactory(McpServerSession.Factory sessionFactory) {
            // nothing to bind in a unit test
        }

        @Override
        public Mono<Void> notifyClients(String method, Object params) {
            return Mono.empty();
        }

        @Override
        public Mono<Void> closeGracefully() {
            return Mono.empty();
        }
    }

    @BeforeEach
    void setUp() throws IOException {
        Path file = dir.resolve("requirements.json");
        Files.writeString(file, """
                {
                  "project": "Test Kata",
                  "requirements": [
                    {
                      "id": "REQ-001",
                      "title": "First",
                      "status": "pending",
                      "story": "As a user...",
                      "acceptanceCriteria": ["Given x, then y"]
                    }
                  ]
                }
                """);
        handlers = new WorkflowToolHandlers(
                () -> RequirementsRepository.load(file),
                new SpecValidator(file, dir),
                new RequirementRefiner(),
                () -> new TestRunSummary(5, 0, 0, 0, List.of()),
                new TddStateMachine());
    }

    private static String text(CallToolResult result) {
        return ((McpSchema.TextContent) result.content().get(0)).text();
    }

    private CallToolResult call(String tool, Map<String, Object> args) {
        SyncToolSpecification spec = McpServerFactory.toolSpecifications(handlers).stream()
                .filter(s -> s.tool().name().equals(tool))
                .findFirst()
                .orElseThrow();
        return spec.callHandler().apply(null, new CallToolRequest(tool, args));
    }

    @Test
    @DisplayName("the factory exposes exactly the seven workflow tools, in workflow order")
    void exposesSevenTools() {
        List<String> names = McpServerFactory.toolSpecifications(handlers).stream()
                .map(spec -> spec.tool().name())
                .toList();
        assertThat(names).containsExactly(
                "list_requirements", "get_requirement", "validate_spec", "refine_requirement",
                "run_tests", "get_tdd_state", "start_refactor");
    }

    @Test
    @DisplayName("every tool wiring delegates to its handler")
    void everyToolDelegates() {
        assertThat(text(call("list_requirements", Map.of()))).contains("REQ-001");
        assertThat(text(call("get_requirement", Map.of("id", "REQ-001")))).contains("workflowHint");
        assertThat(text(call("validate_spec", Map.of()))).contains("\"valid\"");
        assertThat(text(call("refine_requirement", Map.of("id", "REQ-001")))).contains("\"clean\"");
        assertThat(text(call("run_tests", Map.of()))).contains("GREEN");
        assertThat(text(call("get_tdd_state", Map.of()))).contains("phase");
        assertThat(text(call("start_refactor", Map.of("note", "tidy")))).contains("REFACTOR");
    }

    @Test
    @DisplayName("the assembled server carries the workshop identity and tool capability")
    void assembledServerHasIdentityAndCapabilities() {
        McpSyncServer server = McpServerFactory.create(new FakeTransportProvider(), handlers);
        try {
            assertThat(server.getServerInfo().name()).isEqualTo("tdd-workflow-server");
            assertThat(server.getServerInfo().version()).isEqualTo("1.0.0");
            assertThat(server.getServerCapabilities().tools()).isNotNull();
            assertThat(server.getServerCapabilities().logging()).isNotNull();
        } finally {
            server.close();
        }
    }
}
