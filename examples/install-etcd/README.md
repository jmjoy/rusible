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
- `etcd.peer_host` and `etcd.client_host` in the loaded inventory data must be reachable addresses between nodes

The Docker test environment in this repository now provides a minimal `systemd` container setup. It uses `sshd-1` / `sshd-2` / `sshd-3` as the standard container names, and the etcd example uses the in-container addresses of those containers for node-to-node communication. It does not use `privileged`, but it depends on the following runtime conditions:

- Share `/sys/fs/cgroup`
- Provide `tmpfs` mounts for `/run`, `/run/lock`, and `/tmp`
- Run Docker on a host whose cgroup environment is compatible with `systemd` inside containers

If the host or Docker version has weak support for `systemd` inside containers, `systemctl` may still fail. In that case, check cgroup mounts and namespace behavior before resorting to `privileged`.

## Docker Test Environment

Start the three-node test containers:

```bash
docker compose up -d --build
```

Check `systemd` and `ssh`:

```bash
docker compose exec sshd-1 systemctl is-system-running
docker compose exec sshd-1 systemctl status ssh --no-pager
```

## Run

```bash
cargo -Z bindeps run -p install-etcd
```

## Inventory Format

The format follows `hello-inventory`:

- `[vars.etcd]` stores shared deployment parameters
- `[[groups]]` defines the `etcd` host group
- `[[hosts]]` defines SSH connection details and each member's own `etcd.name`, `etcd.peer_host`, and `etcd.client_host`

Only `vars`, `groups`, and `hosts` are valid top-level inventory keys. Rusible
rejects any other top-level key so inventory typos fail fast.

If you pass a directory to `Inventory::from_toml_path`, Rusible recursively
loads every `.toml` file under that directory, sorts them by relative path,
merges `[vars]` tables recursively, and appends `[[groups]]` and `[[hosts]]`.

## License

Licensed under Mulan PSL v2.
