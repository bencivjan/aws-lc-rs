# Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
# SPDX-License-Identifier: Apache-2.0 OR ISC

FROM --platform=linux/amd64 rust:latest

SHELL ["/bin/bash", "-c"]

ARG GO_VERSION=go1.20.1

RUN apt-get update && \
    apt-get -y install clang cmake golang && \
    rustup update && \
    rustup component add rustfmt clippy && \
    cargo install rust-script && \
    git config --global --add safe.directory '*' && \
    go get golang.org/dl/${GO_VERSION} && \
    $HOME/go/bin/${GO_VERSION} download && \
    apt-get autoremove --purge -y && \
    apt-get clean && \
    apt-get autoclean && \
    rm -rf /var/lib/apt/lists/* && \
    rm -rf /tmp/*

ENV PATH="/root/sdk/${GO_VERSION}/bin:${PATH}"
