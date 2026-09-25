# Security Analysis & Vulnerability Scanning

## Overview
Sky Moon Scope now includes comprehensive security scanning for JavaScript vulnerabilities as part of the CI/CD pipeline. This document outlines the security tools, configuration, and best practices.

## Security Tools

### 1. npm audit
Scans dependencies for known vulnerabilities using the npm vulnerability database.

**Local Usage:**
```bash
cd web
npm run security:audit          # Check for high-severity vulnerabilities
npm run security:audit:fix      # Attempt automatic fixes
npm audit --audit-level=high    # Explicit high-level check
```

**CI Integration:**
- Runs as part of the `frontend` CI job
- Fails the job if high-severity vulnerabilities are found
- Runs after ESLint and unit tests

**How it works:**
- Queries npm security database for known CVEs
- Reports vulnerabilities in installed packages and transitive dependencies
- Provides severity levels: critical, high, moderate, low
- Suggests remediation paths and version upgrades

### 2. Semgrep (OWASP + JavaScript Rulesets)
Static analysis tool that scans source code for security patterns and vulnerabilities.

**Local Usage:**
```bash
# Install Semgrep (one-time)
brew install semgrep  # macOS
apt-get install semgrep  # Linux

# Run security analysis
cd web
semgrep --config=p/owasp-top-ten --config=p/javascript .
```

**CI Integration:**
- Runs on every push and PR to `main`
- Uses OWASP Top 10 and JavaScript rulesets
- Generates SARIF reports uploaded to GitHub Code Scanning
- Violations appear as security alerts in the PR

**Configuration:**
- Config file: `.semgrep.yml` (project root)
- Rulesets:
  - `p/owasp-top-ten`: OWASP Top 10 Web Application Security Risks
  - `p/javascript`: JavaScript-specific security patterns

**Covered Vulnerabilities:**
1. **A01: Broken Access Control** - Missing auth checks, permission bypasses
2. **A02: Cryptographic Failures** - Hardcoded secrets, weak crypto
3. **A03: Injection** - SQL/NoSQL injection, command injection, template injection
4. **A04: Insecure Design** - Missing security controls
5. **A05: Security Misconfiguration** - Open CORS, debug mode enabled, exposed endpoints
6. **A06: Vulnerable & Outdated Components** - Known CVEs in dependencies
7. **A07: Identification & Authentication Failures** - Session fixation, weak auth
8. **A08: Software & Data Integrity Failures** - Insecure deserialization, unsafe updates
9. **A09: Logging & Monitoring Failures** - Missing security logging
10. **A10: Server-Side Request Forgery (SSRF)** - Unvalidated URL redirects

## CI/CD Integration

### Frontend Check Job
The `frontend` job in `.github/workflows/ci.yml` now includes three security layers:

```yaml
- name: Audit Dependencies
  run: npm audit --audit-level=high

- name: Run Semgrep Security Analysis
  uses: returntocorp/semgrep-action@v1
  with:
    generateSarif: true
    config: >-
      p/owasp-top-ten
      p/javascript

- name: Upload SARIF Report
  uses: github/codeql-action/upload-sarif@v3
  if: always()
  with:
    sarif_file: semgrep.sarif
```

### GitHub Security Alerts
- SARIF reports are automatically uploaded to GitHub Code Scanning
- Violations appear as security alerts on PRs
- Alerts include file, line number, and remediation guidance

## Addressing Vulnerabilities

### When npm audit Fails
1. **Check the report:**
   ```bash
   npm audit
   ```

2. **Remediation paths:**
   - Automatic: `npm audit fix`
   - Manual: Update package in `package.json` and run `npm install`
   - Dependency issue: Report to package maintainers

3. **Allowlisting (if necessary):**
   Create `.npmauditignore` to skip non-critical advisories (use sparingly):
   ```
   # Format: <advisory-id>
   1234567
   ```

### When Semgrep Flags Issues
1. **Review the finding** - Check the source code and OWASP category
2. **Determine if it's a true positive:**
   - Real vulnerability: Fix the code
   - False positive: Add a `nosemgrep` comment if necessary
3. **Implement fix** - Update source code to eliminate the pattern

**Example nosemgrep usage (only for validated false positives):**
```javascript
// nosemgrep: javascript.security.hardcoded-secret
const TEST_API_KEY = "test_key_for_unit_tests_only";
```

## Best Practices

### Development
1. Run `npm run security:audit` before committing
2. Fix high-severity vulnerabilities immediately
3. Test security fixes thoroughly
4. Keep dependencies updated regularly

### Code Review
1. Verify all Semgrep alerts are addressed in PRs
2. Ensure no hardcoded secrets (API keys, tokens, passwords)
3. Check for unsafe DOM operations (`innerHTML`, `dangerouslySetInnerHTML`)
4. Validate all user inputs before use

### Dependency Management
1. Regularly run `npm audit` locally
2. Use `npm update` to patch vulnerabilities
3. Pin versions in `package-lock.json` (already done via `npm ci`)
4. Review breaking changes before major version updates

## Security Policies
- High-severity vulnerabilities block CI
- Moderate vulnerabilities are reported but don't block
- All security findings must be documented and resolved
- Security updates take priority over feature development

## References
- [OWASP Top 10 2021](https://owasp.org/Top10/)
- [npm audit Documentation](https://docs.npmjs.com/cli/v10/commands/npm-audit)
- [Semgrep Rules Registry](https://semgrep.dev/r)
- [GitHub Code Scanning](https://docs.github.com/en/code-security/code-scanning/automatically-scanning-your-code-for-vulnerabilities-and-errors)

## Questions?
For security concerns or vulnerability reports, refer to the project's [SECURITY.md](../SECURITY.md) or contact the maintainers.
