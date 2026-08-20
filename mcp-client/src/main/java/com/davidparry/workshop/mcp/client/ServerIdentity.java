package com.davidparry.workshop.mcp.client;

/** What the server said about itself during the MCP handshake. */
public record ServerIdentity(String name, String version, String instructions) {
}
