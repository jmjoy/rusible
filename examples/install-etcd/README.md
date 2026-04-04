# install-etcd

`install-etcd` demonstrates how to use Rusible to combine local tasks with inventory-driven remote tasks to complete a TLS-enabled etcd installation flow.

## What It Shows

- Use `Local` to run `delegate_to: localhost` style local tasks
- Use an inventory TOML file or directory to select target hosts in the `etcd` group
- Generate the CA and node certificates through local `shell` and `stat` tasks
- Complete deployment through remote `user`, `file`, `copy`, `unarchive`, `systemd`, `wait_for`, and `command` tasks
- Merge inventory-level variables and host-level variables into the rendering context for the systemd template

## Prerequisites

- `openssl` must be installed on the control host
- Target hosts must be Linux machines that allow root login
- Target hosts must run `systemd`
- `etcd.peer_host` and `etcd.client_host` must resolve to reachable addresses between nodes, whether they come from inventory or from `FactsTask`

The Docker test environment in this repository now provides a minimal `systemd` container setup. The etcd example first gathers `rusible.facts.hostname` from each target and then uses that hostname as the default source for `etcd.peer_host` and `etcd.client_host` when those vars are not set explicitly. It does not use `privileged`, but it depends on the following runtime conditions:

- Share `/sys/fs/cgroup`
- Provide `tmpfs` mounts for `/run`, `/run/lock`, and `/tmp`
- Run Docker on a host whose cgroup environment is compatible with `systemd` inside containers

If the host or Docker version has weak support for `systemd` inside containers, `systemctl` may still fail. If the runtime hostname is not resolvable between nodes, keep `FactsTask` for discovery but override `etcd.peer_host` and `etcd.client_host` explicitly in inventory.

Node certificates are regenerated on each run so the SAN set stays aligned with the current `FactsTask` hostname values even if containers were recreated and their runtime hostnames changed.

## Docker Test Environment

Start the three-node test containers:

```bash
docker compose up -d --build
```

Check `systemd` and `ssh`:

```bash
docker compose exec --index 1 sshd systemctl is-system-running
docker compose exec --index 1 sshd systemctl status ssh --no-pager
```

## Run

```bash
cargo -Z bindeps run -p install-etcd
```

## Inventory Format

The format follows `hello-inventory`:

- `[vars.etcd]` stores shared deployment parameters
- `[[groups]]` defines the `etcd` host group
- `[[hosts]]` defines SSH connection details and each member's own `etcd.name`
- `etcd.peer_host` and `etcd.client_host` are optional host vars; when omitted, the example gathers `rusible.facts.hostname` through `FactsTask` and uses that hostname as the default value
- Explicit `etcd.peer_host` and `etcd.client_host` values still override the gathered hostname when you need a different routable address

Only `vars`, `groups`, and `hosts` are valid top-level inventory keys. Rusible
rejects any other top-level key so inventory typos fail fast.

If you pass a directory to `Inventory::from_toml_path`, Rusible recursively
loads every `.toml` file under that directory, sorts them by relative path,
merges `[vars]` tables recursively, and appends `[[groups]]` and `[[hosts]]`.

## License

Licensed under Mulan PSL v2.
