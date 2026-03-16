FROM ubuntu:24.04

COPY .ssh/rusible_docker_root_ed25519.pub /tmp/rusible_docker_root_ed25519.pub

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends openssh-server \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/run/sshd /root/.ssh \
    && chmod 700 /root/.ssh \
    && cat /tmp/rusible_docker_root_ed25519.pub > /root/.ssh/authorized_keys \
    && chmod 600 /root/.ssh/authorized_keys \
    && echo 'root:123456' | chpasswd \
    && sed -i 's/^#\?PermitRootLogin .*/PermitRootLogin yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PasswordAuthentication .*/PasswordAuthentication yes/' /etc/ssh/sshd_config \
    && sed -i 's/^#\?PubkeyAuthentication .*/PubkeyAuthentication yes/' /etc/ssh/sshd_config \
    && rm -f /tmp/rusible_docker_root_ed25519.pub

EXPOSE 22

CMD ["/usr/sbin/sshd", "-D", "-e"]
