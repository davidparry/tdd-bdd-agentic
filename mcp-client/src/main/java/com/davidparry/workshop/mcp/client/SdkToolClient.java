package com.davidparry.workshop.mcp.client;

import io.modelcontextprotocol.client.McpClient;
import io.modelcontextprotocol.client.McpSyncClient;
import io.modelcontextprotocol.client.transport.ServerParameters;
import io.modelcontextprotocol.client.transport.StdioClientTransport;
import io.modelcontextprotocol.json.McpJsonDefaults;
import io.modelcontextprotocol.spec.McpSchema;

import java.nio.file.Path;
import java.time.Duration;
import java.util.List;
import java.util.Map;

/**
 * {@link McpToolClient} backed by the MCP SDK's synchronous client: launches
 * the workflow server jar as a child process over stdio — exactly what an IDE
 * host does. Pure delegation: every call forwards to the SDK and maps the
 * result through {@link SdkMappers}, which carries all the logic and is
 * covered at 100%. This glue needs a live protocol connection, so it is
 * excluded from the coverage gate alongside the composition root.
 */
public class SdkToolClient implements McpToolClient {

    private final McpSyncClient client;

    public SdkToolClient(Path workshopRoot, Path serverJar) {
        ServerParameters params = ServerParameters.builder("java")
                .args("-Dworkshop.root=" + workshopRoot, "-jar", serverJar.toString())
                .build();
        this.client = McpClient.sync(new StdioClientTransport(params, McpJsonDefaults.getMapper()))
                .requestTimeout(Duration.ofMinutes(6)) // run_tests invokes Maven, give it room
                .clientInfo(new McpSchema.Implementation("tdd-workshop-agent", "1.0.0"))
                .build();
    }

    @Override
    public ServerIdentity initialize() {
        return SdkMappers.toIdentity(client.initialize());
    }

    @Override
    public List<DiscoveredTool> listTools() {
        return SdkMappers.toTools(client.listTools());
    }

    @Override
    public ToolResponse callTool(String name, Map<String, Object> arguments) {
        return SdkMappers.toResponse(client.callTool(SdkMappers.toRequest(name, arguments)));
    }

    @Override
    public void close() {
        client.closeGracefully();
    }
}
