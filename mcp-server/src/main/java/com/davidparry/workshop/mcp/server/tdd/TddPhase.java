package com.davidparry.workshop.mcp.server.tdd;

/**
 * The phases of the Red/Green/Refactor cycle as tracked by the MCP server.
 */
public enum TddPhase {
    /** No test run has been recorded yet. */
    START,
    /** The last test run had failures — write just enough code to pass. */
    RED,
    /** All tests pass — refactor, or pick up the next requirement. */
    GREEN,
    /** A refactor is in progress — re-run the tests to prove it is safe. */
    REFACTOR
}
