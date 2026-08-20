package com.davidparry.workshop.mcp.server.requirements;

/**
 * A spec tree that cannot be resolved — a missing or unparseable file,
 * an include cycle, or an include escaping the spec directory. The
 * message is already formatted the way {@link SpecValidator} reports
 * issues, so it can be surfaced directly.
 */
public class SpecException extends RuntimeException {

    public SpecException(String message) {
        super(message);
    }
}
