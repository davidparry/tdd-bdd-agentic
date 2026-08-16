package com.davidparry.workshop.mcp.server;

import org.junit.platform.suite.api.ConfigurationParameter;
import org.junit.platform.suite.api.IncludeEngines;
import org.junit.platform.suite.api.SelectClasspathResource;
import org.junit.platform.suite.api.Suite;

import static io.cucumber.junit.platform.engine.Constants.GLUE_PROPERTY_NAME;
import static io.cucumber.junit.platform.engine.Constants.PLUGIN_PROPERTY_NAME;

/**
 * Runs the server's own Gherkin feature file through Cucumber on the JUnit
 * Platform. Surefire executes this suite alongside the JUnit unit tests, so
 * {@code mvn test -pl mcp-server} reports one combined result — the same
 * one-bar discipline the server enforces for the kata.
 */
@Suite
@IncludeEngines("cucumber")
@SelectClasspathResource("features")
@ConfigurationParameter(key = GLUE_PROPERTY_NAME, value = "com.davidparry.workshop.mcp.server")
@ConfigurationParameter(key = PLUGIN_PROPERTY_NAME,
        value = "pretty, html:target/cucumber-report.html, json:target/cucumber-report.json")
public class RunCucumberTest {
}
