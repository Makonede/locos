<img width="484" alt="image" src="https://github.com/user-attachments/assets/ed168ce7-958d-4af1-910c-6bfe64213029" />

# locOS

An attempt at a simple Rust operating system.

## Build

Make sure that `make` is installed

```sh
make
```

## Testing

The kernel includes comprehensive tests that require KVM acceleration and x2apic support.

### Local Testing

```sh
make test
```

**Requirements:**
- KVM-capable CPU (Intel VT-x or AMD-V)
- KVM kernel modules loaded
- Access to `/dev/kvm` device
- QEMU with KVM support

### CI Testing

Our tests require KVM acceleration which is not available on GitHub's hosted runners. We use [Actuated](https://actuated.com/) runners that provide KVM support.

**Manual Testing:**
You can manually trigger KVM tests using the "Manual KVM Tests" workflow in the Actions tab.

**For more details on CI setup, see [docs/CI_KVM_SETUP.md](docs/CI_KVM_SETUP.md)**
