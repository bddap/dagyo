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

run: create-local-cluster build-images load-images-into-minikube apply
    
check:
    cargo clippy --all-targets
    cargo fmt -- --check
    cargo test --all
    just build-images


