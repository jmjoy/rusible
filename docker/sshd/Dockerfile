FROM ubuntu:24.04

ENV container=docker

STOPSIGNAL SIGRTMIN+3

COPY .ssh/rusible_docker_root_ed25519.pub /tmp/rusible_docker_root_ed25519.pub

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        dbus \
        iproute2 \
        iputils-ping \
        openssh-server \
        systemd \
        systemd-sysv \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/run/sshd /root/.ssh /etc/systemd/system/multi-user.target.wants \
    && chmod 700 /root/.ssh \
    && cat /tmp/rusible_docker_root_ed25519.pub > /root/.ssh/authorized_keys \
    && chmod 600 /root/.ssh/authorized_keys \
    && echo 'root:123456' | chpasswd \
    && sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?UsePAM .*/UsePAM yes/' /etc/ssh/sshd_config \
    && ln -sf /lib/systemd/system/ssh.service /etc/systemd/system/multi-user.target.wants/ssh.service \
    && rm -f /etc/systemd/system/getty.target.wants/* \
    && rm -f /etc/systemd/system/sysinit.target.wants/systemd-tmpfiles-setup.service \
    && rm -f /lib/systemd/system/multi-user.target.wants/* \
    && ln -sf /lib/systemd/system/ssh.service /lib/systemd/system/multi-user.target.wants/ssh.service \
    && truncate -s 0 /etc/machine-id \
    && rm -f /tmp/rusible_docker_root_ed25519.pub

EXPOSE 22

CMD ["/sbin/init"]
