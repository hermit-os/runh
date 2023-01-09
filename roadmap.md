# `runh` issues, missing features, extension possibilities
## `runh` setup
- Docker:
  - edit /etc/docker/daemon.json:  
    Add `runtimes` list:
    ```
    "runtimes": {
      "runh": {
        "path": "<path-to-runh>",
        "runtimeArgs": [
          "-l",
          "debug",
          "--hermit-env",
          "<path-to-hermit-environment>",
          "--debug-log",
          "--debug-config"
        ]
      }
    }
    ```
  - use `--runtime=runh` in Docker commands
- Kubernetes
  - Set up CRI-O:  
    edit /etc/crio/crio.conf  
    Extend the `runtimes` table:
    ```
    [crio.runtime.runtimes.runh]
      runtime_path = "<path-to-runh>"
      runtime_type = "oci"
      runtime_root = "/run/runh" # Will be used as --root argument to runh
    ```
  - Set up containerd:
    - TODO: Refer to https://github.com/containerd/containerd/blob/main/docs/cri/config.md
  
  - Kubernetes `RuntimeClass` (requires either CRI-O or containerd setup):  
    ```
    apiVersion: node.k8s.io/v1
    kind: RuntimeClass
    metadata:
      name: hermit  # RuntimeClass name
    handler: runh  # The name of the corresponding CRI configuration
    ```
  - Specify `runtimeClassName: hermit` in any pod spec to use the Hermit container runtime

- Hermit environment
  - Requires an unpacked Linux base image with a microVM-compatible version of QEMU installed
  - Current test image: registry.git.rwth-aachen.de/jonas.schroeder1/hermit-registry/ubuntu-base:latest
  - Test Dockerfile:  
    ```
    FROM ubuntu:latest
    RUN apt update
    RUN apt install -y --no-install-recommends qemu-system-x86 qemu-system-x86-microvm
    ```
  - For running images that do not contain the RustyHermit-Bootloader, the Hermit environment needs to contain the bootloader binary named `rusty-loader` in the same folder that the application binary will be mounted into. If the image contains the app at `/hermit/app`, the bootloader has to be in `<hermit-env>/hermit/rusty-loader`.  
  Code: https://github.com/JTS22/runh/blob/5ea768750af4d0e53f4357ece26dd39e9866f3a0/src/init.rs#L651-L670

# Issues
- Networking
  - works rather inconsistent (might be because of RustyHermit network code)
  - network namespace is not reset when restarting a Kubernetes Pod -> usage of dummy device in network.rs
  - when a Hermit-Container runs in a pod, other containers in this pod likely can't access the network
    - running two Hermit-Containers in one pod should not be possible
- Rootfs
  - create.rs creates an overlay on top of the overlay provided by the container manager.
    This might fail for some configurations in the container manager (the normal overlayfs driver seems to work though)
  - because the container manager's overlay is mounted read-only in the new overlay, everything written to the rootfs from inside the 
    container ends up in a temporary folder and can not be detected / saved by the container manager
  - the whole overlay-creation can fail if the `runh` project root lies on a filesystem that does not support overlays
- Entering the container
  - To prevent CVE-2019-5736, `runc` first clones the entire runtime binary before doing anything in `runc init` (see https://github.com/opencontainers/runc/blob/657ed0d4a0ce3c46e202ef54e6baf0d5e88f2c01/libcontainer/nsenter/nsexec.c#L859-L865). `runh` currently does not do that and is therefore vulnerable to this kind of attacks
  - `runc` does some more operations in the `nsexec.c` that are currently not done by `runh`
  - The process clone in init.rs currently uses the unsafe libc code. Maybe this can be done using nix instead
  - The cloned child gets assigned a 32KB memory region from the parent heap as its stack. I have no idea if this is still valid after the parent exits and if the final container process after the `exec`-call is still linked to this stack region.  
  Code: https://github.com/JTS22/runh/blob/5ea768750af4d0e53f4357ece26dd39e9866f3a0/src/init.rs#L187-L208
- Container deletion:
  - `runh` cannot delete all the remaining container processes, because there is no cgroup to get all the processes in a container
- Error reporting / logging:
  - For CRI-O, log is written to stdout and appears at the start of container / pod logs
  - When `runh init` crashes, this is not detected by `runh create` until the next read from the init pipe, leading to `runh create` crashing with a rather uninformative panic message
- Container images
  - when `runh` detects a Hermit-App, it does not check any other files in the image. By providing a file named `qemu-system-x86_64` in the image, a Hermit-Image can run arbitrary code in a container (in the same way, Linux images can). This might be a feature (e.g. to allow users to
  package their own version of QEMU) or a security issue (when `runh` makes assumptions based on the fact that only the QEMU VM will run in any given Hermit container)
  - Are there licensing issues with providing the Hermit Environment base image (containing Ubuntu + QEMU files) on some RWTH registry?


# Missing features:
- User namespaces
  - These require a second clone during the container entering process. During this, the parent also has to set up the child's UID and GID mappings
- Hooks
  - Currently, only the (deprecated, but used by Docker) prestart hooks are run. All other hooks are ignored
  - Hook timeouts are unimplemented
- cgroups (in their entirety)
- process resource restrictions
- seccomp restrictions
- filesystem namespace finalization (https://github.com/opencontainers/runc/blob/657ed0d4a0ce3c46e202ef54e6baf0d5e88f2c01/libcontainer/init_linux.go#L138-L203)
  - changing to the requested CWD
  - changing to the correct user
  - apply process capabilities
- automatic setup of the Hermit Environment
- multiple small things that are marked with `TODO` in the `runh` code

# Extension possibilities
- a `runh exec` command to spawn additional processes inside the container
- starting the QEMU-virtiofsd file system daemon in the container to expose some of the mounted filesystem to the virtual machine
- allowing the user to customize more VM-related options (resources, microVM, ...) either through annotations or by configuring the container image
- better network setup
- applying process resource restrictions set by Kubernetes to the VM
- checkpointing / container backup and restore
- running Linux and Hermit containers in the same pod (this might already work to some degree)
- an "attached" mode where `runh` itself can attach to the container console, like `runc` can when just using `runc run` on the command line.


