# Security Policy

## Supported Versions

The following versions of Talaria are currently supported with security updates:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1.0 | :x:                |

## Reporting a Vulnerability

The Talaria team takes security issues seriously. We appreciate your efforts to responsibly disclose your findings.

### Where to Report

**DO NOT** create public GitHub issues for security vulnerabilities.

Instead, please report security vulnerabilities via one of these methods:

1. **Email**: security@talaria-project.org
2. **GitHub Security Advisories**: [Report a vulnerability](https://github.com/talaria/talaria/security/advisories/new)

### What to Include

When reporting a vulnerability, please include:

- **Description**: Clear description of the vulnerability
- **Impact**: Potential impact and severity assessment
- **Steps to Reproduce**: Detailed steps to reproduce the issue
- **Proof of Concept**: Code or commands demonstrating the vulnerability
- **Affected Versions**: List of affected Talaria versions
- **Suggested Fix**: If you have suggestions for fixing the issue

### What to Expect

1. **Acknowledgment**: We'll acknowledge receipt within 48 hours
2. **Initial Assessment**: Within 5 business days, we'll provide:
   - Initial severity assessment
   - Estimated timeline for a fix
   - Any immediate mitigation steps

3. **Updates**: We'll keep you informed about:
   - Progress on the fix
   - Release timeline
   - Credit attribution preferences

4. **Resolution**: Once fixed:
   - Security advisory will be published
   - Fix will be released in a patch version
   - You'll be credited (unless you prefer anonymity)

## Security Best Practices

When using Talaria in production:

### Input Validation

- Always validate FASTA input files before processing
- Use the built-in sanitization features
- Limit file sizes appropriately for your system

### File System Security

```bash
# Set appropriate permissions
chmod 700 ~/.talaria
chmod 600 ~/.talaria/config.toml

# Use separate directories for untrusted input
export TALARIA_WORKSPACE_DIR=/tmp/talaria-sandbox
```

### Configuration Security

```toml
# config.toml
[security]
max_input_size = "10GB"
enable_sandboxing = true
validate_checksums = true

[workspace]
auto_cleanup = true
preserve_on_failure = false
```

### Network Security

When using cloud storage:

```bash
# Use encrypted connections
export TALARIA_CHUNK_SERVER="https://..."  # Not http://

# Use authentication
export AWS_PROFILE="talaria-readonly"
export GOOGLE_APPLICATION_CREDENTIALS="/path/to/service-account.json"
```

### Container Security

When using Docker:

```dockerfile
# Use specific version tags
FROM talaria/talaria:0.1.0

# Run as non-root user
USER talaria

# Use read-only filesystem where possible
docker run --read-only talaria/talaria
```

## Known Security Considerations

### Memory Usage

Large databases can consume significant memory:
- Monitor memory usage during reduction
- Set appropriate limits
- Use streaming mode for very large files

### Temporary Files

Talaria creates temporary files during processing:
- Ensure `/tmp` has sufficient space
- Temporary files are cleaned automatically
- Set `TALARIA_PRESERVE_ON_FAILURE=0` in production

### External Tool Integration

When using external aligners:
- Verify tool binaries with checksums
- Use official sources for downloads
- Keep tools updated

## Security Updates

### Update Notifications

Subscribe to security updates:
- Watch the GitHub repository
- Subscribe to security advisories
- Follow release notes

### Applying Updates

```bash
# Check current version
talaria --version

# Update from source
git pull
cargo build --release

# Or update binary
curl -L https://github.com/talaria/talaria/releases/latest/download/talaria-linux-x86_64.tar.gz | tar xz
```

## Vulnerability Disclosure Policy

We follow responsible disclosure practices:

1. **Private Disclosure**: Vulnerabilities are kept private until fixed
2. **Patch Development**: We develop and test patches privately
3. **Coordinated Release**: We coordinate with reporters on disclosure timing
4. **Public Disclosure**: After patch release, details are made public
5. **Credit**: Security researchers are credited (with permission)

## Security Features

Talaria includes several security features:

### Integrity Verification

- SHA256 checksums for all chunks
- Merkle DAG verification
- Content-addressed storage

### Data Protection

- No sensitive data in logs
- Secure temporary file handling
- Automatic cleanup of workspaces

### Input Sanitization

- FASTA format validation
- Sequence alphabet verification
- Header injection prevention

## Compliance

Talaria can be configured for various compliance requirements:

### HIPAA Compliance

When processing medical data:
- Enable audit logging
- Use encrypted storage
- Implement access controls

### GDPR Compliance

When processing EU data:
- Enable data deletion
- Implement audit trails
- Document data processing

## Contact

For security-related questions:
- **Security Team**: security@talaria-project.org
- **General Questions**: [GitHub Discussions](https://github.com/talaria/talaria/discussions)
- **Bug Reports**: [GitHub Issues](https://github.com/talaria/talaria/issues) (non-security)

## Acknowledgments

We thank the security researchers who have helped improve Talaria:
- Security advisories are listed in our [GitHub Security tab](https://github.com/talaria/talaria/security)
- Researchers are credited in our [Hall of Fame](https://github.com/talaria/talaria/security/hall-of-fame)

---

**Remember**: Security is everyone's responsibility. If you see something, say something!