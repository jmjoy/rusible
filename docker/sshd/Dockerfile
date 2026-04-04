FROM ubuntu:24.04

ENV container=docker
ENV DEBIAN_FRONTEND=noninteractive

STOPSIGNAL SIGRTMIN+3

RUN apt-get update && apt-get install -y --no-install-recommends \
    systemd \
    systemd-sysv \
    openssh-server \
    ca-certificates \
    curl \
    dbus \
    iproute2 \
    iputils-ping \
    && rm -rf /var/lib/apt/lists/*

RUN mkdir -p /etc/systemd/system/ssh.service.d \
    && echo "[Service]\nRuntimeDirectory=sshd\nRuntimeDirectoryMode=0755" > /etc/systemd/system/ssh.service.d/override.conf

COPY .ssh/rusible_docker_root_ed25519.pub /tmp/pubkey
RUN mkdir -p /root/.ssh && chmod 700 /root/.ssh \
    && cat /tmp/pubkey > /root/.ssh/authorized_keys \
    && chmod 600 /root/.ssh/authorized_keys \
    && rm -f /tmp/pubkey \
    && echo 'root:123456' | chpasswd

RUN sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication yes/' /etc/ssh/sshd_config

RUN find /etc/systemd/system/multi-user.target.wants/ -type l -not -name 'ssh.service' -delete \
    && rm -f /lib/systemd/system/multi-user.target.wants/getty.target \
    && rm -f /etc/systemd/system/getty.target.wants/* \
    && systemctl enable ssh

RUN truncate -s 0 /etc/machine-id

EXPOSE 22

CMD ["/sbin/init"]
