#!/usr/bin/env bash
# Presenter preflight for the 60-minute workshop.
# Run from anywhere inside the repo, ideally T-30 minutes before going on stage:
#   scripts/preflight.sh
# Exits non-zero if any check fails.

set -u

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PASS=0
FAIL=0

ok()   { printf '  \033[32mPASS\033[0m  %s\n' "$1"; PASS=$((PASS + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1"; printf '        fix: %s\n' "$2"; FAIL=$((FAIL + 1)); }

echo "Workshop preflight — $ROOT"
echo

# 1. Java 21+
JAVA_MAJOR="$(java -version 2>&1 | awk -F'"' '/version/ {split($2, v, "."); print v[1]}')"
if [ -n "${JAVA_MAJOR:-}" ] && [ "$JAVA_MAJOR" -ge 21 ] 2>/dev/null; then
    ok "Java $JAVA_MAJOR (need 21+)"
else
    bad "Java 21+ not found (got: ${JAVA_MAJOR:-none})" "install a Java 21+ JDK and put it on PATH"
fi

# 2. Maven
if command -v mvn >/dev/null 2>&1; then
    ok "Maven $(mvn -version 2>/dev/null | head -1 | awk '{print $3}')"
else
    bad "Maven not on PATH" "install Maven 3.9+"
fi

# 3. Full build (also produces both jars)
if mvn -B -q clean package >/tmp/preflight-build.log 2>&1; then
    ok "mvn clean package is green"
else
    bad "build failed" "see /tmp/preflight-build.log"
fi

# 4. Jars exist
for jar in mcp-server/target/tdd-mcp-server.jar mcp-client/target/tdd-agent.jar; do
    if [ -f "$jar" ]; then
        ok "$jar present"
    else
        bad "$jar missing" "run: mvn -q package"
    fi
done

# 5. Cucumber suite ran (BDD layer alive)
if ls kata/target/surefire-reports/TEST-*RunCucumberTest.xml >/dev/null 2>&1; then
    ok "Cucumber suite executed (surefire report found)"
else
    bad "no Cucumber surefire report in kata/target/surefire-reports" \
        "check kata/src/test/java/.../RunCucumberTest.java and the cucumber dependencies"
fi

# 6. End-to-end smoke: client launches server, drives all six steps
if java -jar mcp-client/target/tdd-agent.jar >/tmp/preflight-agent.log 2>&1; then
    if grep -q '"phase" : "GREEN"' /tmp/preflight-agent.log; then
        ok "end-to-end agent run reports GREEN"
    else
        bad "agent ran but did not report GREEN" "see /tmp/preflight-agent.log"
    fi
else
    bad "end-to-end agent run failed" "see /tmp/preflight-agent.log"
fi

# 7. Demo not burned: REQ-003 must still be pending with no scenario written
REQ3_STATUS="$(python3 -c "
import json
reqs = json.load(open('requirements/requirements.json'))['requirements']
print(next(r['status'] for r in reqs if r['id'] == 'REQ-003'))
" 2>/dev/null)"
if [ "$REQ3_STATUS" = "pending" ]; then
    ok "REQ-003 status is pending"
else
    bad "REQ-003 status is '${REQ3_STATUS:-unreadable}'" "reset: git checkout -- kata requirements"
fi
if grep -q '@REQ-003' kata/src/test/resources/features/string_calculator.feature; then
    bad "feature file already contains an @REQ-003 scenario (rehearsal leftover)" \
        "reset: git checkout -- kata requirements && mvn -q package"
else
    ok "feature file has no REQ-003 scenario yet"
fi

# 8. Slides present (open them once manually to warm the CDN cache)
if [ -f slides/index.html ]; then
    ok "slides/index.html present — open it once now to cache the reveal.js CDN assets"
else
    bad "slides/index.html missing" "restore it from git"
fi

echo
echo "Result: $PASS passed, $FAIL failed."
if [ "$FAIL" -gt 0 ]; then
    echo "NOT ready. Fix the failures above and re-run."
    exit 1
fi
echo "Ready. Remaining manual steps: open the slides once, confirm the"
echo "tdd-workflow server shows green in Cursor's MCP settings, clear the agent chat."
