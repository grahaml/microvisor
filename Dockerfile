FROM python:3.12-slim
ENV DEBIAN_FRONTEND=noninteractive

# Install networking tools and util-linux (provides runuser for dropping privileges)
RUN apt-get update && apt-get install -y \
    curl \
    git \
    iproute2 \
    iputils-ping \
    net-tools \
    util-linux \
    && rm -rf /var/lib/apt/lists/*

# Install the agent framework
RUN pip install --no-cache-dir hermes-agent

# Create the unprivileged agent user and their workspace
RUN useradd -m -s /bin/bash agent_user && \
    mkdir -p /home/agent_user/.hermes && \
    chown -R agent_user:agent_user /home/agent_user

# Setup metadata mount point and ensure agent_user can read it if needed
RUN mkdir -p /mnt/metadata

# Copy the init script
COPY scripts/init-hermes.sh /usr/local/bin/init-hermes.sh
RUN chmod +x /usr/local/bin/init-hermes.sh

# Firecracker will run this script as PID 1 (root) at boot
ENTRYPOINT ["/usr/local/bin/init-hermes.sh"]
