# CI Setup for KVM-Dependent Tests

This document explains how to set up CI for running kernel tests that require KVM acceleration and x2apic support.

## Problem

GitHub's hosted runners do not support nested virtualization or KVM acceleration. Our kernel tests require:
- KVM acceleration (`-enable-kvm`)
- x2apic support (`-cpu host,+x2apic`) 
- Hardware acceleration (`-machine q35,accel=kvm`)

Software emulation cannot provide x2apic support, so we need actual KVM acceleration.

## Solutions

### Option 1: Actuated (Recommended)

[Actuated](https://actuated.com/) provides GitHub Actions runners with KVM support.

**Pros:**
- Drop-in replacement for GitHub hosted runners
- Secure and isolated (each job runs in its own VM)
- Supports nested virtualization and KVM
- No infrastructure management required

**Cons:**
- Paid service
- Currently only supports x86_64 (no ARM64)

**Setup:**
1. Sign up at [actuated.com](https://actuated.com/pricing)
2. Register your GitHub organization
3. Use `runs-on: actuated-4cpu-8gb` in your workflow
4. The main workflow file (`.github/workflows/rust.yml`) is already configured for Actuated

**Cost:** Check [actuated.com/pricing](https://actuated.com/pricing) for current rates.

### Option 2: Self-Hosted Runner

Set up your own runner on bare metal with KVM support.

**Pros:**
- Full control over the environment
- Can be cost-effective for high usage
- Supports any hardware configuration

**Cons:**
- Requires infrastructure management
- Security considerations for public repos
- Potential for job conflicts

**Setup:**

1. **Prepare the host machine:**
   ```bash
   # Install required packages
   sudo apt-get update
   sudo apt-get install -y qemu-system-x86 xorriso curl

   # Verify KVM support
   sudo apt-get install -y cpu-checker
   kvm-ok

   # Add runner user to kvm group
   sudo usermod -a -G kvm $USER
   ```

2. **Install GitHub Actions runner:**
   ```bash
   # Download and configure runner (replace with your repo URL and token)
   mkdir actions-runner && cd actions-runner
   curl -o actions-runner-linux-x64-2.311.0.tar.gz -L https://github.com/actions/runner/releases/download/v2.311.0/actions-runner-linux-x64-2.311.0.tar.gz
   tar xzf ./actions-runner-linux-x64-2.311.0.tar.gz
   ./config.sh --url https://github.com/YOUR_ORG/locos --token YOUR_TOKEN
   ```

3. **Set up as a service:**
   ```bash
   sudo ./svc.sh install
   sudo ./svc.sh start
   ```

4. **Use the self-hosted workflow:**
   - Rename `.github/workflows/rust-self-hosted.yml` to `.github/workflows/rust.yml`
   - Or create a separate workflow for self-hosted testing

### Option 3: Alternative CI Platforms

Consider CI platforms that support nested virtualization:

**GitLab CI:**
- GitLab.com shared runners don't support KVM
- Self-managed GitLab runners can support KVM
- Similar setup to GitHub self-hosted runners

**Buildkite:**
- Supports bring-your-own-infrastructure
- Can run on KVM-capable hosts

**Drone CI:**
- Can run on your own infrastructure
- Supports Docker and bare metal runners

## Testing the Setup

Regardless of which solution you choose, verify KVM support with:

```bash
# Check KVM device
ls -la /dev/kvm

# Check KVM modules
lsmod | grep kvm

# Test QEMU with x2apic
qemu-system-x86_64 -enable-kvm -cpu host,+x2apic -machine q35,accel=kvm -nographic -serial none -monitor none &
sleep 2
killall qemu-system-x86_64
```

## Security Considerations

**For Public Repositories:**
- GitHub recommends against self-hosted runners for public repos
- Actuated provides better isolation and security
- Consider using branch protection rules

**For Private Repositories:**
- Self-hosted runners are generally safe
- Ensure proper network isolation
- Keep runner software updated

## Recommendation

For most use cases, **Actuated is the recommended solution** because:
1. It provides the security of hosted runners with KVM support
2. No infrastructure management required
3. Drop-in replacement for existing workflows
4. Professional support available

The main workflow is already configured for Actuated - you just need to sign up and register your organization.
