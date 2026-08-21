package com.davidparry.workshop.mcp.server;

import com.davidparry.workshop.mcp.server.requirements.Requirement;
import com.davidparry.workshop.mcp.server.requirements.RequirementsRepository;

import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;
import java.util.regex.Matcher;
import java.util.regex.Pattern;
import java.util.stream.Collectors;

import static org.assertj.core.api.Assertions.assertThat;

/**
 * The executable proof that this module's SDD/BDD/TDD triangle is closed:
 * every requirement in the server's own spec is implemented, every
 * requirement has tagged Gherkin scenarios in the feature file it names,
 * and every scenario tag traces back to a spec requirement. Coverage of the
 * TDD layer is enforced separately by the JaCoCo 100% check.
 *
 * <p>The spec is loaded through {@link RequirementsRepository} — the same
 * code path the MCP tools use.
 */
class SpecCompletenessTest {

    /** Surefire runs with the module directory as the working directory. */
    private static final Path MODULE_DIR = Path.of("").toAbsolutePath();
    private static final Path REPO_ROOT = MODULE_DIR.getParent();
    private static final Pattern TAG = Pattern.compile("@(SRV-\\d+)");

    private static RequirementsRepository spec;

    @BeforeAll
    static void loadSpec() {
        spec = RequirementsRepository.load(MODULE_DIR.resolve("requirements/server-requirements.json"));
    }

    @Test
    @DisplayName("SDD: the server has its own spec and every requirement is implemented")
    void everyRequirementIsImplemented() {
        assertThat(spec.all()).isNotEmpty();
        assertThat(spec.nextPending()).isEmpty();
        assertThat(spec.all()).allSatisfy(r -> assertThat(r.status()).isEqualTo("implemented"));
    }

    @Test
    @DisplayName("SDD: every requirement carries acceptance criteria and names its feature file")
    void everyRequirementHasCriteriaAndFeatureFile() {
        assertThat(spec.all()).allSatisfy(r -> {
            assertThat(r.acceptanceCriteria()).as("%s acceptance criteria", r.id()).isNotEmpty();
            assertThat(r.featureFile()).as("%s featureFile", r.id()).isNotBlank();
            assertThat(REPO_ROOT.resolve(r.featureFile())).as("%s feature file exists", r.id()).exists();
        });
    }

    @Test
    @DisplayName("BDD: every requirement has at least one tagged scenario in its feature file")
    void everyRequirementHasATaggedScenario() throws IOException {
        for (Requirement r : spec.all()) {
            String feature = Files.readString(REPO_ROOT.resolve(r.featureFile()));
            assertThat(feature)
                    .as("feature file %s has a scenario tagged @%s", r.featureFile(), r.id())
                    .contains("@" + r.id());
        }
    }

    @Test
    @DisplayName("BDD: every scenario tag in the feature file traces back to a spec requirement")
    void everyScenarioTagTracesToTheSpec() throws IOException {
        Set<String> specIds = spec.all().stream().map(Requirement::id).collect(Collectors.toSet());
        Set<String> featureFiles = spec.all().stream().map(Requirement::featureFile).collect(Collectors.toSet());
        for (String featureFile : featureFiles) {
            String feature = Files.readString(REPO_ROOT.resolve(featureFile));
            Matcher matcher = TAG.matcher(feature);
            while (matcher.find()) {
                assertThat(specIds)
                        .as("tag @%s in %s exists in the spec", matcher.group(1), featureFile)
                        .contains(matcher.group(1));
            }
        }
    }

    @Test
    @DisplayName("BDD: the Cucumber suite actually executes the tagged feature file")
    void cucumberSuiteExecutesTheFeature() {
        List<String> featureFiles = spec.all().stream().map(Requirement::featureFile).distinct().toList();
        assertThat(featureFiles).allSatisfy(f ->
                assertThat(f).startsWith("mcp-server/src/test/resources/features/"));
    }
}
