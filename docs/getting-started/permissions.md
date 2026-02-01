# Accessibility Permissions

AXTerminator uses the macOS Accessibility API to interact with applications. This requires explicit permission from the user.

## Granting Permissions

### macOS Ventura and later (13+)

1. Open **System Settings**
2. Navigate to **Privacy & Security** → **Accessibility**
3. Click the **+** button
4. Add your terminal app (Terminal, iTerm2, VS Code, etc.)
5. Toggle the switch to enable

### macOS Monterey (12)

1. Open **System Preferences**
2. Navigate to **Security & Privacy** → **Privacy** → **Accessibility**
3. Click the lock icon and authenticate
4. Click **+** and add your terminal app
5. Ensure the checkbox is checked

## Checking Permission Status

```python
import axterminator as ax

if ax.is_accessibility_enabled():
    print("Accessibility enabled!")
else:
    print("Please grant accessibility permissions")
    # Optionally prompt the user
    ax.request_accessibility()
```

## Common Issues

### "Operation not permitted"

This means accessibility permissions haven't been granted. Follow the steps above.

### Permission granted but still not working

Try these steps:

1. Remove and re-add the app in Accessibility settings
2. Restart the terminal/IDE
3. Restart your Mac (if issues persist)

### Running in CI/CD

GitHub Actions macOS runners don't have GUI access. Use environment variables to skip integration tests:

```yaml
env:
  CI: true
  SKIP_INTEGRATION: true
```

## Security Note

Accessibility permissions are powerful - they allow an app to control other applications. Only grant permissions to trusted software.
