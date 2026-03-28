#################################################################
FROM scratch AS binaries

ADD binaries/mediamtx_*_linux_amd64.tar.gz /linux/amd64
ADD binaries/mediamtx_*_linux_armv6.tar.gz /linux/arm/v6
ADD binaries/mediamtx_*_linux_armv7.tar.gz /linux/arm/v7
ADD binaries/mediamtx_*_linux_arm64.tar.gz /linux/arm64

#################################################################
FROM debian:stable-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN ARCH=$(dpkg --print-architecture) && \
    case "$ARCH" in \
        amd64)   GRPCURL_ARCH="linux_x86_64" ;; \
        arm64)   GRPCURL_ARCH="linux_arm64" ;; \
        armhf)   GRPCURL_ARCH="linux_armv7" ;; \
        armel)   GRPCURL_ARCH="linux_armv6" ;; \
        *) echo "Unsupported arch: $ARCH" && exit 1 ;; \
    esac && \
    curl -sSL "https://github.com/fullstorydev/grpcurl/releases/latest/download/grpcurl_${GRPCURL_ARCH}.tar.gz" \
    | tar -xz -C /usr/local/bin grpcurl

RUN curl --version

ARG TARGETPLATFORM
COPY --from=binaries /$TARGETPLATFORM /

ENTRYPOINT [ "/mediamtx" ]