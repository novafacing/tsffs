ARG BASEIMG="amr-registry.caas.intel.com/tsffs/tsffs:latest"

FROM ${BASEIMG}

RUN dnf -y install 'dnf-command(config-manager)' && \
    dnf -y config-manager --add-repo https://cli.github.com/packages/rpm/gh-cli.repo && \
    dnf -y update && \
    dnf -y install gh

RUN mkdir -p /tsffs
COPY start-tsffs.sh /tsffs
COPY start-tsffs.simics /tsffs
