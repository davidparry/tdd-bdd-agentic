package com.davidparry.workshop.mcp.client;

/** The text of a tool call result and whether the server flagged it as an error. */
public record ToolResponse(String text, boolean error) {
}
