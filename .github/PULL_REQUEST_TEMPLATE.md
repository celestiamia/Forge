# Pull Request Template

## Description

<!-- Describe your changes in detail -->

## Related Issue

<!-- Link to the issue this PR addresses (e.g., "Fixes #123", "Closes #456") -->

## Type of Change

- [ ] Bug fix (non-breaking change that fixes an issue)
- [ ] New feature (non-breaking change that adds functionality)
- [ ] Breaking change (fix or feature that would cause existing code to not compile/run)
- [ ] Performance improvement
- [ ] Refactoring (no functional changes)
- [ ] Documentation update
- [ ] Test addition/update
- [ ] Build/CI change

## Targets Affected

- [ ] x86_64-unknown-linux-gnu (primary)
- [ ] x86_32-unknown-linux-gnu
- [ ] x86_16-boot

## Testing

### Tests Added/Modified

<!-- List any new tests or modified tests -->

### Test Results

```
# Paste output of relevant test runs
$ cargo test --test integration
...
```

### Manual Verification

<!-- If applicable, describe manual testing steps and results -->

## Checklist

- [ ] Code compiles without warnings (`cargo build --release`)
- [ ] All tests pass (`cargo test`)
- [ ] Integration tests pass (`cargo test --test integration`)
- [ ] 32-bit tests pass (if target affected)
- [ ] Boot sector tests pass (if target affected)
- [ ] New functionality has test coverage
- [ ] Documentation updated (README, code comments)
- [ ] Commit messages follow conventional format
- [ ] No unrelated changes (whitespace, formatting in untouched files)
- [ ] `Cargo.lock` updated if dependencies changed

## Screenshots/Output (if applicable)

<!-- For visual changes, codegen output, etc. -->

## Additional Notes

<!-- Any other information, concerns, or context for reviewers -->