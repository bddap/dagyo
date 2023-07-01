# https://github.com/casey/just

create-local-cluster:
    minikube start --container-runtime=containerd

delete-local-cluster:
    minikube delete

build-images:
    docker build --build-arg BIN=dagyo-sidecar -t dagyo-sidecar -f all.dockerfile .
    docker build --build-arg BIN=dagyo-operator -t dagyo-operator -f all.dockerfile .
    docker build --build-arg BIN=dagyo-scheduler -t dagyo-scheduler -f all.dockerfile .

load-images-into-minikube:
    minikube image load dagyo-sidecar
    minikube image load dagyo-operator
    minikube image load dagyo-scheduler

apply:
    minikube kubectl -- apply -f k8s-manifest.yaml

_maybe_upload content_addressed_image:
    #!/usr/bin/env bash
    set -euo pipefail
    minikube image ls > /dev/null
    if minikube image ls | grep {{content_addressed_image}} > /dev/null; then
        echo skipping {{content_addressed_image}} > /dev/stderr
    else      
        echo uploading {{content_addressed_image}} > /dev/stderr
        minikube image load {{content_addressed_image}}
    fi

load-sample-images-into-minikube:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo run --bin dagyo-build -- ./sample-progdefs/vertspec.toml \
      | jq -r '.spec.executors | to_entries | .[] | .value | .image' \
      | xargs -I{} just _maybe_upload {}

set-dagyo-cluster-config:
    cargo run --bin dagyo-build -- ./sample-progdefs/vertspec.toml --name samples | minikube kubectl -- apply -f -

run: create-local-cluster build-images load-images-into-minikube apply load-sample-images-into-minikube set-dagyo-cluster-config
    
check:
    cargo clippy --all-targets
    cargo fmt -- --check
    cargo test --all
    just build-images

   
# some useful tools for debugging include:
# kdash or k9s - tui kubernetes dashboards


