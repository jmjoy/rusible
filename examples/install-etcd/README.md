# install-etcd

`install-etcd` 演示如何用 Rusible 组合本地任务和 inventory 驱动的远端任务，完成一个带 TLS 的 etcd 安装流程。

## What It Shows

- 使用 `Local` 执行 `delegate_to: localhost` 风格的本地任务
- 使用 `inventory.toml` 选择 `etcd` 组目标主机
- 通过本地 `shell` 与 `stat` 任务生成 CA 与节点证书
- 通过远端 `user`、`file`、`copy`、`unarchive`、`template`、`systemd`、`wait_for` 与 `command` 任务完成部署
- 将 inventory 级变量与主机级变量合并进 systemd 模板渲染上下文

## Prerequisites

- 控制端需要安装 `openssl`
- 目标主机需要是 root 可登录的 Linux 主机
- 目标主机需要运行 `systemd`
- `inventory.toml` 里的 `etcd.peer_host` 和 `etcd.client_host` 必须是节点间可互通的地址

仓库里的 Docker 测试环境现在提供了一版最小 `systemd` 容器方案，默认使用 `sshd-1` / `sshd-2` / `sshd-3` 作为通用容器名，etcd 示例也使用这些容器内地址做节点互联。它不使用 `privileged`，但依赖如下运行条件：

- 共享 `/sys/fs/cgroup`
- 为 `/run`、`/run/lock`、`/tmp` 提供 `tmpfs`
- Docker 主机本身使用可供容器内 `systemd` 使用的 cgroup 环境

如果宿主机或 Docker 版本对容器内 `systemd` 支持较差，`systemctl` 仍可能失败；此时优先检查 cgroup 挂载和 namespace 行为，而不是先加 `privileged`。

## Docker Test Environment

启动三节点测试容器：

```bash
docker compose up -d --build
```

检查 `systemd` 和 `ssh`：

```bash
docker compose exec sshd-1 systemctl is-system-running
docker compose exec sshd-1 systemctl status ssh --no-pager
```

## Run

```bash
cargo -Z bindeps run -p install-etcd
```

## Inventory Format

格式沿用 `hello-inventory`：

- `[vars.etcd]` 放共享部署参数
- `[[groups]]` 定义 `etcd` 主机组
- `[[hosts]]` 定义 SSH 连接信息和每个成员自己的 `etcd.name` / `etcd.peer_host` / `etcd.client_host`

## License

Licensed under Mulan PSL v2.
