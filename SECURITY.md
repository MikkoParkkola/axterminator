# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.9.x   | Yes                |
| 0.8.x   | Yes                |
| 0.7.x   | Yes                |
| < 0.7   | No                 |

## Reporting a Vulnerability

If you discover a security vulnerability in AXTerminator, please report it responsibly.

**Email**: mikko.parkkola@iki.fi

**Do not** open a public GitHub issue for security vulnerabilities.

### What to include

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

### Timeline

- **48 hours**: Acknowledgement of your report
- **7 days**: Initial assessment and severity classification
- **30 days**: Fix developed and tested (for confirmed vulnerabilities)

### Scope

The following are in scope for security reports:

- **Accessibility API misuse**: Unintended access to UI elements outside the target application
- **Sandbox escapes**: Bypassing macOS security boundaries through the accessibility API
- **Credential handling**: Insecure storage or transmission of API keys (VLM backends)
- **Code injection**: Ability to execute arbitrary code through crafted element queries or recordings

### Automated checks

Every pull request and push to `main` runs automated dependency security checks:

- `cargo audit` against the RustSec advisory database
- `cargo deny check advisories bans sources` for supply-chain policy
- Weekly Dependabot updates for Cargo dependencies and GitHub Actions

These checks complement, but do not replace, manual validation for macOS Accessibility/TCC-gated flows that cannot be exercised on GitHub-hosted runners.

### Out of Scope

- Denial of service against the local machine (macOS accessibility permissions already grant broad access)
- Issues requiring physical access to an unlocked machine with accessibility permissions already granted

## Acknowledgements

We appreciate responsible disclosure and will credit reporters in the changelog (unless anonymity is requested).
