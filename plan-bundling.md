# Plan: Containerise pi-agent for k3s

## Goal

Run `pi-agent` as a Kubernetes **DaemonSet** — one pod per Pi node — inside the
existing k3s cluster managed by `../pi-cluster`.

---

## Shape of the solution

| Concern | Approach |
|---------|----------|
| Pod type | DaemonSet (one instance per node) |
| Port binding | `hostNetwork: true` — port 8765 binds on each node's host NIC |
| Process visibility | `hostPID: true` + host `/proc` mount |
| Hardware sensors | Host `/sys` mount (hwmon, device-tree) |
| Device access | Host `/dev` mount, `privileged: true` |
| OS identity | Mount host `/etc/os-release` at `/etc/os-release` |
| Username resolution | Mount host `/etc/passwd` at `/etc/passwd` |
| Deployment | Ansible in `../pi-cluster/40-app-setup/` applies the manifest |

---

## Dockerfile

Minimal `debian:bookworm-slim` base; only the pre-built binary is copied in.
Build with `docker buildx --platform linux/arm64` or natively on the Pi.

```dockerfile
FROM debian:bookworm-slim
COPY target/aarch64-unknown-linux-gnu/release/pi-agent /usr/local/bin/pi-agent
EXPOSE 8765
ENTRYPOINT ["/usr/local/bin/pi-agent"]
```

Image distribution options (to be decided):
- Push to a registry (GHCR, Docker Hub, local registry in cluster)
- Save as OCI tar (`docker buildx --output type=oci`) and import via Ansible:
  `sudo k3s ctr images import pi-agent.tar`

---

## Known complexities to prototype

### 1. vcgencmd (CPU freq, voltage, throttle flags)

`vcgencmd` depends on Pi-specific shared libraries (`libvchiq_arm.so`,
`libbcm_host.so`) that are not in standard Debian images.

Options to explore:
- **Mount binary + libs from host** — mount `/usr/bin/vcgencmd` and
  `/usr/lib/aarch64-linux-gnu/libvchiq_arm.so` etc. from host; fragile if
  paths vary across Pi OS versions.
- **Install in image via Pi apt repo** — add
  `http://archive.raspberrypi.org/debian/` to the Dockerfile; works when
  built for `linux/arm64` under QEMU or on-device.
- **Accept unavailability** — the collector already returns zero/defaults when
  `vcgencmd` fails; freq/voltage/throttle fields are blank in the TUI.

### 2. hailortcli / hailo_perf_query

Hailo tools are installed on the control node via `../pi-cluster/20-hailo-setup/`.
Same problem as vcgencmd: binary + shared libs are host-only.

Options to explore:
- Mount `/usr/bin/hailortcli` and Hailo libs from host.
- Accept unavailability on worker nodes (only control node has Hailo anyway).

### 3. Disk usage (`df -Pk`)

Inside the container `df` only sees container mounts, not host filesystems.

Options to explore:
- Mount host `/` read-only at `/host`; teach collector to prefix paths or
  run `df -Pk /host`.
- Parse `/proc/mounts` + `statfs()` directly instead of shelling out to `df`.

### 4. Image distribution without a registry

k3s uses containerd, not Docker. To load a locally-built image:
```sh
docker buildx build --platform linux/arm64 \
  --output type=oci,dest=pi-agent.tar \
  -f pi-agent/Dockerfile .

# via Ansible on each node:
sudo k3s ctr images import pi-agent.tar
```
A local registry (e.g. `registry:2` container already in k3s) avoids the tar
copy step and is worth setting up if image iteration is frequent.

---

## Makefile targets to add (after prototype)

```
make save-agent          # build OCI tar → pi-agent.tar
make push-agent          # build and push to REGISTRY=…
```

Remove `make deploy-agent` (Ansible handles deployment).

---

## Prototype checklist

- [ ] Build minimal Dockerfile; confirm binary runs on Pi
- [ ] Write DaemonSet manifest with host mounts
- [ ] Verify `/proc`, `/sys`, `/dev` mounts give correct data
- [ ] Test vcgencmd: does it work with `/dev` bind-mount + privileged?
- [ ] Test Hailo detection via `/dev/hailo0`
- [ ] Test disk reporting — confirm `df` limitation; pick fix
- [ ] Decide on image distribution strategy (registry vs tar import)
- [ ] Add Ansible role to `../pi-cluster/40-app-setup/`
