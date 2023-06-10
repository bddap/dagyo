# https://github.com/casey/just

start-local-cluster:
    minikube start --container-runtime=containerd

build-images:
    docker build --build-arg BIN=dagyo-sidecar -t dagyo-sidecar -f all.dockerfile .
    docker build --build-arg BIN=dagyo-operator -t dagyo-operator -f all.dockerfile .
    docker build --build-arg BIN=dagyo-scheduler -t dagyo-scheduler -f all.dockerfile .

load-images-into-minikube:
    minikube image load dagyo-sidecar
    minikube image load dagyo-operator
    minikube image load dagyo-scheduler

register:
    cargo run --bin dagyo-operator -- --crd | minikube kubectl -- apply -f -
    cargo run --bin dagyo-operator -- --deployment | minikube kubectl -- apply -f -

run: start-local-cluster build-images load-images-into-minikube register
    
check:
    cargo clippy --all-targets
    cargo fmt -- --check
    cargo test --all
    just build-images

