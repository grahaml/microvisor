FROM python:3.12-slim
ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y \
	curl \
	git \
	iproute2 \
	iputils-ping \
	net-tools \
	&& rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir hermes-agent
 
COPY scripts/init-hermes.sh /usr/local/bin/init-hermes.sh

RUN chmod +x /usr/local/bin/init-hermes.sh

ENTRYPOINT ["/usr/local/bin/init-hermes.sh"]




