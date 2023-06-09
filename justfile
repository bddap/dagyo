# https://github.com/casey/just

start-local-cluster:
    minikube start --container-runtime=containerd

build-images:
    docker build --build-arg BIN=dagyo-sidecar -t dagyo-sidecar -f all.dockerfile .
    docker build --build-arg BIN=dagyo-operator -t dagyo-operator -f all.dockerfile .

check:
    cargo clippy --all-targets
    cargo fmt -- --check
    cargo test --all
    just build-images
