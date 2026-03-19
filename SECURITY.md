# Security Policy

## Supported Versions

| Version | Supported          |
|---------|--------------------|
| 0.4.x   | Yes                |
| < 0.4   | No                 |

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

### Out of Scope

- Denial of service against the local machine (macOS accessibility permissions already grant broad access)
- Issues requiring physical access to an unlocked machine with accessibility permissions already granted

## Acknowledgements

We appreciate responsible disclosure and will credit reporters in the changelog (unless anonymity is requested).
