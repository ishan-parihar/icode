FROM rust:bookworm

RUN apt-get update && apt-get install -y \
    ca-certificates \
    git \
    libssl-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Bind-mount workflow: repo is mounted at /workspace
# Avoids baking source into image
ENV CARGO_TARGET_DIR=/tmp/icode-target

WORKDIR /workspace

CMD ["/bin/bash"]
